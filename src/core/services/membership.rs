use std::{collections::HashMap, sync::Arc};

use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{AccountCreateParams, AccountDescribeParams, AccountKind},
            list::{ListResponse, RequestFilterParams},
            membership::{
                Membership, MembershipCreateParams, MembershipDeleteParams,
                MembershipDescribeParams, MembershipListParams, MembershipUpdateParams,
            },
            permission::PermissionRule,
            profile::{ProfileCreateParams, ProfileDeleteParams, ProfileMeta},
            role::{Role, RoleDescribeParams, RoleFilter, RoleListParams},
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            account::AccountService, validator::AuthValidator, permission::CANONICAL_PERMISSIONS,
            profile::ProfileService, role::RoleService, workspace::WorkspaceService,
        },
        traits::{
            list::RequestListParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::{
        contains::FilterByContains,
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::{
            account::{AccountFilter, AccountForCreate, AccountMeta},
            id::DbId,
            membership::{
                JoinedRoleOnMembership, MembershipFilter, MembershipForCreate, MembershipForUpdate,
                MembershipRow, MembershipStatus, MembershipWithRoles,
            },
        },
        join::{GetManyToMany, LinkManyToMany, ListManyToMany},
        manager::StoreManager,
        stores::membership::MembershipStore,
        traits::{crud::*, dbx::DbExecutor},
    },
};

pub struct MembershipService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    acc_svc: Arc<AccountService<D, C>>,
    profile_svc: Arc<ProfileService<D, C>>,
    role_svc: Arc<RoleService<D, C>>,
    validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for MembershipService<D, C> {
    type CoreModel = Membership;

    type ServiceStore = MembershipStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.membership
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }

    fn validator(&self) -> &AuthValidator {
        self.validator.as_ref()
    }

    fn is_scoped(&self) -> bool {
        true
    }
}

impl<D: DbExecutor, C: CacheExecutor> MembershipService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        acc_svc: Arc<AccountService<D, C>>,
        profile_svc: Arc<ProfileService<D, C>>,
        role_svc: Arc<RoleService<D, C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            acc_svc,
            profile_svc,
            role_svc,
            validator,
        }
    }

    pub fn set_cm(&mut self, cm: Arc<CacheManager<C>>) {
        self.cm = cm.clone()
    }

    /// Reads the workspace's configured `default_membership_status` from the raw
    /// row (the `Workspace -> WorkspaceRow` conversion discards the real config).
    ///
    /// The workspace table is global (no `workspace_id` column), so the read goes
    /// through an unscoped store context.
    async fn default_status_for_workspace(
        &self,
        ctx: &CoreCtx,
        workspace_id: Uuid,
    ) -> CoreResult<MembershipStatus> {
        let store_ctx = ctx.unscoped_store_ctx();
        let row = self
            .sm
            .workspace
            .get(&store_ctx, &workspace_id.into())
            .await?;
        Ok(row.config.default_membership_status)
    }

    /// Resolves the account for `email` (creating it if needed) and ensures a
    /// profile exists for `(account_id, workspace_id)`, returning the resolved
    /// `account_id` and linked `profile_id`.
    ///
    /// All account/profile access goes through the public `AccountService` and
    /// `ProfileService` APIs (no direct store access for those tables).
    async fn resolve_account_and_profile(
        &self,
        ctx: &mut CoreCtx,
        email: &str,
        workspace_id: Uuid,
    ) -> CoreResult<(Uuid, Option<Uuid>)> {
        // 1. Resolve or create the account.
        let account_id = match self
            .acc_svc
            .get_by_email(
                ctx,
                &AccountDescribeParams {
                    id: None,
                    email: Some(email.to_string()),
                },
            )
            .await?
        {
            Some(account) => account.id,
            None => {
                let account = self
                    .acc_svc
                    .create(
                        ctx,
                        AccountCreateParams {
                            email: email.to_string(),
                            name: email.to_string(),
                            kind: AccountKind::User,
                            enabled: true,
                            verified: false,
                            description: None,
                            avatar_url: None,
                            tags: None,
                            meta: None,
                        },
                    )
                    .await?;
                account.id
            }
        };

        // 2. Ensure a profile exists for (account_id, workspace_id).
        //    Escalate the profile permissions the helpers validate against.
        ctx.escalate_perms(&[
            CANONICAL_PERMISSIONS.profile.create,
            CANONICAL_PERMISSIONS.profile.describe,
        ])?;

        let profile_id = match self
            .profile_svc
            .find_by_account_workspace(ctx, account_id, workspace_id)
            .await?
        {
            Some(profile) => profile.id,
            None => {
                let created = self
                    .profile_svc
                    .create(
                        ctx,
                        ProfileCreateParams {
                            account_id,
                            workspace_id,
                            name: email.to_string(),
                            description: None,
                            display_name: None,
                            job_title: None,
                            timezone: None,
                            avatar_url: None,
                            tags: vec![],
                            meta: ProfileMeta::default(),
                        },
                    )
                    .await;

                match created {
                    Ok(profile) => profile.id,
                    // Concurrent create won the race — fetch the existing profile.
                    Err(CoreError::AlreadyExists(_)) => self
                        .profile_svc
                        .find_by_account_workspace(ctx, account_id, workspace_id)
                        .await?
                        .ok_or_else(|| {
                            CoreError::AlreadyExists(
                                "profile missing after duplicate create".to_string(),
                            )
                        })?
                        .id,
                    Err(err) => return Err(err),
                }
            }
        };

        Ok((account_id, Some(profile_id)))
    }

    async fn get_roles(
        &self,
        ctx: &mut CoreCtx,
        roles: Vec<JoinedRoleOnMembership>,
    ) -> CoreResult<Vec<Role>> {
        let mut data = vec![];

        ctx.escalate_perms(&["role:describe"]);

        // TODO: optimize query, use dedicated role service method
        // to list roles from list of ids

        for role in roles {
            let role = self
                .role_svc
                .describe(
                    ctx,
                    RoleDescribeParams {
                        id: role.id.into(),
                        workspace_id: role.workspace_id.into(),
                    },
                )
                .await?;
            data.push(role);
        }

        Ok(data)
    }

    /// Hydrates membership rows into [`Membership`] models.
    ///
    /// Output goes through `Membership::from_with_roles`, which surfaces
    /// `account_id` + `profile_id` but never the account email — the account
    /// entity (incl. email) is intentionally not fetched here (T017, email
    /// privacy).
    async fn hydrate_memberships(
        &self,
        ctx: &mut CoreCtx,
        rows: Vec<MembershipWithRoles>,
    ) -> CoreResult<Vec<Membership>> {
        let mut roles_map: HashMap<Uuid, Role> = HashMap::new();

        let mut data: Vec<Membership> = Vec::with_capacity(rows.len());

        // Hydrate results
        for row in rows.into_iter() {
            // build role hashmap, this prevents too many describe calls if needed
            // this is very naive, best is custom store method with
            // custom SQL
            let mut membership_roles: Vec<Role> = vec![];

            let roles = self.get_roles(ctx, row.roles).await?;

            let membership = Membership::from_with_roles(row.membership, membership_roles);

            data.push(membership);
        }

        Ok(data)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for MembershipService<D, C> {
    type CreateParams = MembershipCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.membership.create;

    /// Creates a membership and optionally associates it with roles.
    ///
    /// Two resolution paths are supported:
    /// - `params.email` — resolves (or creates) the account, ensures a profile
    ///   exists in the workspace, then links the membership to that profile.
    /// - `params.account_id` — legacy direct link (profile resolved elsewhere;
    ///   `profile_id` may be supplied explicitly).
    ///
    /// When the caller does not specify a `status`, the workspace config's
    /// `default_membership_status` is applied.
    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: MembershipCreateParams,
    ) -> CoreResult<Membership> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::CREATE_PERMISSION])
            .await?;

        // Extract fields needed after params is consumed by resolution.
        let workspace_id = params.workspace_id;
        let role_ids = params.role_ids.clone();
        let scope = params.scope;
        let project_id = params.project_id;
        let tags = params.tags;
        let meta = params.meta;

        // --- Resolve (account_id, profile_id) ---
        let (account_id, profile_id) = if let Some(email) = &params.email {
            self.resolve_account_and_profile(ctx, email, workspace_id).await?
        } else {
            let account_id = params.account_id.ok_or_else(|| {
                CoreError::InvalidParams(
                    "exactly one of 'email' or 'account_id' must be provided".to_string(),
                )
            })?;
            (account_id, params.profile_id)
        };

        // --- Resolve effective status: explicit param wins, else workspace config default ---
        let effective_status = match params.status {
            Some(status) => status,
            None => self.default_status_for_workspace(ctx, workspace_id).await?,
        };

        // Guard: one membership per account per workspace
        let membership_filter: MembershipFilter = json!({
            "account_id": account_id.to_string(),
            "workspace_id": workspace_id.to_string()
        })
        .try_into()?;

        let existing = store
            .list(&store_ctx, Some(membership_filter), None)
            .await?;

        if !existing.is_empty() {
            return Err(CoreError::AlreadyExists(format!(
                "membership already exists for account '{}' in workspace '{}'",
                account_id, workspace_id
            )));
        }

        let m_create = MembershipForCreate {
            account_id,
            workspace_id,
            profile_id,
            scope,
            status: effective_status,
            project_id,
            tags,
            meta,
        };

        let membership_row = store.create(&store_ctx, m_create).await?;

        // Assign roles to the new membership
        let role_db_ids: Vec<DbId> = role_ids.iter().map(|id| DbId::from(*id)).collect();
        self.sm
            .membership
            .set_many_to_many_links(&store_ctx, &membership_row.id, role_db_ids)
            .await?;

        self.describe(
            ctx,
            MembershipDescribeParams {
                id: membership_row.id.into(),
                workspace_id: membership_row.workspace_id.into(),
            },
        )
        .await
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for MembershipService<D, C> {
    type DescribeParams = MembershipDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.membership.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: MembershipDescribeParams,
    ) -> CoreResult<Membership> {
        let store = self.store();
        let db_id: DbId = params.id.into();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DESCRIBE_PERMISSION])
            .await?;

        // Get Membership with Roles (Join query)
        let membership_with_roles: MembershipWithRoles =
            store.get_many_to_many(&store_ctx, &db_id).await?;

        // TODO: implement membership -> role -> permission join query
        let roles = self.get_roles(ctx, membership_with_roles.roles).await?;
        let membership = Membership::from_with_roles(membership_with_roles.membership, roles);

        Ok(membership)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for MembershipService<D, C> {
    type ListParams = MembershipListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.membership.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::LIST_PERMISSION])
            .await?;

        let options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;
        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_many_to_many(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let data = self.hydrate_memberships(ctx, data).await?;

        Ok(ListResponse::new(data, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for MembershipService<D, C> {
    type UpdateParams = MembershipUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.membership.update;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.membership.describe]);

        let cur = self
            .describe(
                ctx,
                MembershipDescribeParams {
                    id: params.id,
                    workspace_id: params.workspace_id,
                },
            )
            .await?;

        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::UPDATE_PERMISSION])
            .await?;

        let new_version = cur.version + 1;

        if let Some(role_ids) = &params.role_ids {
            let role_ids = role_ids.iter().map(|e| e.into()).collect();
            store
                .set_many_to_many_links(&store_ctx, &cur.id.into(), role_ids)
                .await?;
        }

        let res = store
            .update(
                &store_ctx,
                &params.id.into(),
                params.into_store_params(new_version),
            )
            .await?;

        self.cm.auth.invalidate(res.id.into()).await?;

        // TODO(T032): Push notification trigger — notify all workspace clients
        // that a membership changed. Requires wiring a `ClientService`
        // dependency into `MembershipService` (constructor + factory). Then
        // call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "membership_changed",
        //         serde_json::json!({ "membership_id": res.id, "account_id": ... }),
        //         ctx,
        //     ).await;
        self.describe(
            ctx,
            MembershipDescribeParams {
                id: res.id.into(),
                workspace_id: res.workspace_id.into(),
            },
        )
        .await
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for MembershipService<D, C> {
    type DeleteParams = MembershipDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.membership.delete;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DELETE_PERMISSION])
            .await?;

        let to_delete = self
            .describe(
                ctx,
                MembershipDescribeParams {
                    id: params.id,
                    workspace_id: params.workspace_id,
                },
            )
            .await?;

        // NOTE: all permission linked to this role are unlinked ON DELETE CASCADE on the membership_role table

        let res = store.delete(&store_ctx, &params.id.into()).await?;
        self.cm.auth.invalidate(res.id.into()).await?;

        // Cleanup: the profile follows the membership (1:1 for now). When the
        // last membership referencing a profile is removed, delete the
        // now-orphaned profile — the workspace identity is recreated if the
        // member is added again.
        if let Some(profile_id) = to_delete.profile_id {
            let profile_filter: MembershipFilter = json!({
                "profile_id": profile_id.to_string()
            })
            .try_into()?;

            let remaining = self
                .sm
                .membership
                .list(&store_ctx, Some(profile_filter), None)
                .await?;

            if remaining.is_empty() {
                ctx.escalate_perms(&[CANONICAL_PERMISSIONS.profile.delete])?;
                self.profile_svc
                    .delete(
                        ctx,
                        ProfileDeleteParams {
                            id: profile_id,
                            workspace_id: to_delete.workspace_id,
                        },
                    )
                    .await?;
            }
        }

        // TODO(T032): Push notification trigger — notify all workspace clients
        // that a membership was deleted. Requires wiring a `ClientService`
        // dependency into `MembershipService`. Then call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "membership_changed",
        //         serde_json::json!({ "membership_id": to_delete.id, "account_id": ... }),
        //         ctx,
        //     ).await;
        Ok(to_delete)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{manager::CacheManager, mock::MockChx},
        config::Config,
        core::services::registry::ServiceRegistry,
        store::{
            dbx::MockDbx,
            entities::{
                account::AccountRow,
                audit::AuditFields,
                membership::{
                    MembershipMeta, MembershipRow, MembershipScope, MembershipStatus,
                    MembershipWithRoles,
                },
                profile::{ProfileMeta, ProfileRow},
                workspace::WorkspaceRow,
            },
        },
    };
    use serial_test::serial;
    use uuid::Uuid;

    /// Builds a `MembershipService` backed by an in-memory `MockDbx` + `MockChx`.
    fn mock_svc(dbx: MockDbx) -> Arc<MembershipService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.membership.clone()
    }

    fn ws_row(ws_id: Uuid) -> WorkspaceRow {
        WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        }
    }

    fn account_row(account_id: Uuid) -> AccountRow {
        AccountRow {
            id: account_id.into(),
            ..Default::default()
        }
    }

    fn profile_row(profile_id: Uuid, account_id: Uuid, ws_id: Uuid) -> ProfileRow {
        ProfileRow {
            id: profile_id.into(),
            account_id,
            workspace_id: ws_id,
            name: "email-resolved-profile".to_string(),
            version: 1,
            meta: ProfileMeta {
                schema_version: "1".to_string(),
            },
            ..Default::default()
        }
    }

    fn membership_row(mem_id: Uuid, account_id: Uuid, ws_id: Uuid) -> MembershipRow {
        MembershipRow {
            id: mem_id.into(),
            account_id,
            workspace_id: ws_id,
            profile_id: None,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            version: 1,
            tags: vec![],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields::default(),
        }
    }

    fn membership_with_roles(mem_id: Uuid, account_id: Uuid, ws_id: Uuid) -> MembershipWithRoles {
        let row = membership_row(mem_id, account_id, ws_id);
        MembershipWithRoles {
            id: row.id,
            membership: row,
            roles: vec![],
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_create() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // create -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // create -> duplicate guard -> list (no existing membership)
            .with_all::<MembershipRow>(vec![])
            // create -> store.create
            .with_one::<MembershipRow>(membership_row(mem_id, account_id, ws_id))
            // create -> describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // create -> describe -> get_many_to_many (count)
            .with_one::<(i64,)>((0,))
            // create -> describe -> get_many_to_many
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // create -> describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        let params = MembershipCreateParams {
            account_id: Some(account_id),
            email: None,
            workspace_id: ws_id,
            role_ids: vec![],
            scope: MembershipScope::Workspace,
            status: Some(MembershipStatus::Active),
            profile_id: None,
            project_id: None,
            tags: vec!["pioneer".to_string()],
            meta: MembershipMeta::default(),
        };

        // -- Execute
        let membership = svc.create(&mut ctx, params).await?;

        // -- Assert
        assert_eq!(membership.id, mem_id);
        assert_eq!(membership.workspace_id, ws_id);
        assert_eq!(membership.account_id, account_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_create_by_email() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();
        let email = "new-member@example.com".to_string();

        let dbx = MockDbx::new()
            // resolve -> get_by_email (account does not exist)
            .with_optional::<AccountRow>(None)
            // account.create -> duplicate-email check (still absent)
            .with_optional::<AccountRow>(None)
            // account.create -> INSERT ... RETURNING
            .with_one::<AccountRow>(account_row(account_id))
            // profile find -> list (no existing profile)
            .with_all::<ProfileRow>(vec![])
            // profile.create -> duplicate guard -> list (none)
            .with_all::<ProfileRow>(vec![])
            // profile.create -> INSERT ... RETURNING
            .with_one::<ProfileRow>(profile_row(profile_id, account_id, ws_id))
            // default status -> workspace.get (config default -> Invited)
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // membership duplicate guard -> list (none)
            .with_all::<MembershipRow>(vec![])
            // membership store.create
            .with_one::<MembershipRow>(MembershipRow {
                profile_id: Some(profile_id),
                status: MembershipStatus::Invited,
                ..membership_row(mem_id, account_id, ws_id)
            })
            // create -> describe -> get_many_to_many (count)
            .with_one::<(i64,)>((0,))
            // create -> describe -> get_many_to_many
            .with_optional::<MembershipWithRoles>(Some(MembershipWithRoles {
                id: mem_id.into(),
                membership: MembershipRow {
                    profile_id: Some(profile_id),
                    status: MembershipStatus::Invited,
                    ..membership_row(mem_id, account_id, ws_id)
                },
                roles: vec![],
            }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        let params = MembershipCreateParams {
            account_id: None,
            email: Some(email),
            workspace_id: ws_id,
            role_ids: vec![],
            scope: MembershipScope::Workspace,
            status: None,
            profile_id: None,
            project_id: None,
            tags: vec![],
            meta: MembershipMeta::default(),
        };

        // -- Execute
        let membership = svc.create(&mut ctx, params).await?;

        // -- Assert
        assert_eq!(membership.id, mem_id);
        assert_eq!(membership.workspace_id, ws_id);
        assert_eq!(membership.account_id, account_id);
        assert_eq!(membership.profile_id, Some(profile_id));
        // status falls back to the workspace config default when not specified
        assert_eq!(membership.status, MembershipStatus::Invited);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_create_duplicate() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // duplicate guard -> list finds an existing membership
            .with_all::<MembershipRow>(vec![membership_row(Uuid::new_v4(), account_id, ws_id)]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        let params = MembershipCreateParams {
            account_id: Some(account_id),
            email: None,
            workspace_id: ws_id,
            role_ids: vec![],
            scope: MembershipScope::Workspace,
            status: Some(MembershipStatus::Active),
            profile_id: None,
            project_id: None,
            tags: vec![],
            meta: MembershipMeta::default(),
        };

        // -- Execute
        let err = svc.create(&mut ctx, params).await;

        // -- Assert
        assert!(
            matches!(err, Err(CoreError::AlreadyExists(_))),
            "creating a duplicate membership must fail with AlreadyExists"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_describe() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute
        let membership = svc
            .describe(
                &mut ctx,
                MembershipDescribeParams {
                    id: mem_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        // -- Assert
        assert_eq!(membership.id, mem_id);
        assert_eq!(membership.workspace_id, ws_id);
        assert_eq!(membership.account_id, account_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_list() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // list -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // list_many_to_many -> count
            .with_one::<(i64,)>((1,))
            // list_many_to_many -> data
            .with_all::<MembershipWithRoles>(vec![membership_with_roles(mem_id, account_id, ws_id)])
            // count_with_tags_and_filter -> count
            .with_one::<(i64,)>((1,))
            // hydrate -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute
        let res = svc
            .list(
                &mut ctx,
                MembershipListParams {
                    workspace_id: ws_id,
                    filter: None,
                    options: None,
                },
            )
            .await?;

        // -- Assert
        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id, mem_id);
        assert_eq!(res.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_update() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> get_many_to_many (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)))
            // update -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // update -> store.update
            .with_optional::<MembershipRow>(Some(membership_row(mem_id, account_id, ws_id)))
            // update -> describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> get_many_to_many (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute
        let updated = svc
            .update(
                &mut ctx,
                MembershipUpdateParams {
                    id: mem_id,
                    workspace_id: ws_id,
                    tags: Some(vec!["veteran".to_string()]),
                    ..Default::default()
                },
            )
            .await?;

        // -- Assert
        assert_eq!(updated.id, mem_id);
        assert_eq!(updated.workspace_id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_delete() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // delete -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // delete -> describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> get_many_to_many (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)))
            // delete -> store.delete
            .with_optional::<MembershipRow>(Some(membership_row(mem_id, account_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute
        let deleted = svc
            .delete(
                &mut ctx,
                MembershipDeleteParams {
                    id: mem_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        // -- Assert
        assert_eq!(deleted.id, mem_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_delete_cascades_profile_cleanup() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let mw_with_profile = {
            let row = MembershipRow {
                profile_id: Some(profile_id),
                ..membership_row(mem_id, account_id, ws_id)
            };
            MembershipWithRoles {
                id: row.id,
                membership: row,
                roles: vec![],
            }
        };

        let dbx = MockDbx::new()
            // delete -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // delete -> describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> get_many_to_many (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many
            .with_optional::<MembershipWithRoles>(Some(mw_with_profile))
            // delete -> store.delete
            .with_optional::<MembershipRow>(Some(MembershipRow {
                profile_id: Some(profile_id),
                ..membership_row(mem_id, account_id, ws_id)
            }))
            // cleanup -> membership list by profile (no remaining memberships)
            .with_all::<MembershipRow>(vec![])
            // cleanup -> profile.delete -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // cleanup -> profile.delete -> guard -> membership list (empty)
            .with_all::<MembershipRow>(vec![])
            // cleanup -> profile.delete -> store.delete
            .with_optional::<ProfileRow>(Some(profile_row(profile_id, account_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute
        let deleted = svc
            .delete(
                &mut ctx,
                MembershipDeleteParams {
                    id: mem_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        // -- Assert
        assert_eq!(deleted.id, mem_id);
        assert_eq!(deleted.profile_id, Some(profile_id));

        Ok(())
    }
}

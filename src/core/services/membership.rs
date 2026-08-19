use std::{collections::HashMap, sync::Arc};

use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::AccountDescribeParams,
            list::{ListResponse, RequestFilterParams},
            membership::{
                Membership, MembershipCreateParams, MembershipDeleteParams,
                MembershipDescribeParams, MembershipListParams, MembershipUpdateParams,
                MembershipWithPolicies,
            },
            permission::PermissionRule,
            profile::ProfileDeleteParams,
            role::{Role, RoleDescribeParams, RoleFilter, RoleListParams},
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            account::AccountService, permission::CANONICAL_PERMISSIONS, profile::ProfileService,
            role::RoleService, validator::AuthValidator, workspace::WorkspaceService,
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
    /// entity (incl. email) is intentionally not fetched here (email
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

            let membership =
                Membership::from_with_roles_and_policies(row.membership, membership_roles, vec![]);

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
    /// The membership always resolves a concrete account and workspace profile:
    /// - `email` is required and (at the web layer) validated/normalized. It is
    ///   used as the profile's email, which may differ from the account email.
    /// - `account_id`, when provided, resolves an existing account by id
    ///   (returning `NotFound` if absent); otherwise the account is resolved by
    ///   `email`, creating a fresh account if needed.
    /// - `profile`, when provided, supplies persona details applied only when a
    ///   NEW profile is created for `(account_id, workspace_id)`.
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        // Extract fields needed after params is consumed.
        // Resolve the concrete workspace: derive from the context when omitted.
        let workspace_id = params.workspace_id.unwrap_or_else(|| ctx.scoped_ws_id());
        let account_id = params.account_id;
        let profile_id = params.profile_id;
        let role_ids = params.role_ids.clone();
        let policy_ids = params.policy_ids.clone();
        let scope = params.scope;
        let project_id = params.project_id;
        let tags = params.tags;
        let meta = params.meta;

        // Membership creation consumes an existing profile; ownership is
        // checked here for a clear domain error and enforced again by SQL.
        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.profile.describe])?;
        let profile = self
            .profile_svc
            .describe(
                ctx,
                crate::core::models::profile::ProfileDescribeParams {
                    id: Some(profile_id),
                    email: None,
                    workspace_id: Some(workspace_id),
                },
            )
            .await?;
        if profile.account_id != account_id || profile.workspace_id != workspace_id {
            return Err(CoreError::InvalidParams(
                "profile does not belong to the requested account and workspace".to_string(),
            ));
        }

        // --- Resolve effective status: explicit param wins, else workspace config default ---
        let effective_status = match params.status {
            Some(status) => status,
            None => self.default_status_for_workspace(ctx, workspace_id).await?,
        };

        // Guard: one membership per account per workspace
        // TODO: future must allow multiple memberships across projects within the workspace
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

        // Assign policies to the new membership
        let policy_db_ids: Vec<DbId> = policy_ids.iter().map(|id| DbId::from(*id)).collect();
        store
            .set_many_to_many_policies(&store_ctx, &membership_row.id, policy_db_ids)
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        // Get Membership with Roles (Join query)
        let membership_with_roles: MembershipWithRoles =
            store.get_many_to_many(&store_ctx, &db_id).await?;

        // TODO: implement membership -> role -> permission join query
        let roles = self.get_roles(ctx, membership_with_roles.roles).await?;

        // Get Membership with Policies (Join query)
        let membership_with_policies = store.get_many_to_many_policies(&store_ctx, &db_id).await?;
        let policies = membership_with_policies
            .policies
            .into_iter()
            .map(|el| el.into())
            .collect();

        let membership = Membership::from_with_roles_and_policies(
            membership_with_roles.membership,
            roles,
            policies,
        );

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
            .scope_and_validate(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // TODO: implement policy-permission validation.
        // A member mutating their OWN membership (e.g. leaving the workspace)
        // should be gated by the seeded "self" policy; this check is stubbed out
        // for now. Standard permission checks above remain active.
        // if cur.account_id == ctx.account_id() {
        //     self.validator().validate_policy(
        //         ctx.policy_set(),
        //         "membership:update",
        //         "self",
        //         Some("membership.account.id === user.id"),
        //     )?;
        // }

        let new_version = cur.version + 1;

        if let Some(role_ids) = &params.role_ids {
            let role_ids = role_ids.iter().map(|e| e.into()).collect();
            store
                .set_many_to_many_links(&store_ctx, &cur.id.into(), role_ids)
                .await?;
        }

        // `policy_ids`, when present, replaces the membership's policy links
        // (set semantics).
        if let Some(policy_ids) = &params.policy_ids {
            let policy_ids = policy_ids.iter().map(|e| e.into()).collect();
            store
                .set_many_to_many_policies(&store_ctx, &cur.id.into(), policy_ids)
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

        // Role/policy link changes (and the version bump) alter the
        // membership's resolved PolicySet, so the policy cache entry is stale
        // after any membership update.
        self.cm.policy.invalidate(res.id.into()).await?;

        // TODO: Push notification trigger — notify all workspace clients
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
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

        // The membership no longer exists — drop its policy cache entry too.
        self.cm.policy.invalidate(res.id.into()).await?;
        // Cleanup: the profile follows the membership (1:1 for now). When the
        // last membership referencing a profile is removed, delete the
        // now-orphaned profile — the workspace identity is recreated if the
        // member is added again.
        {
            let profile_id = to_delete.profile_id;
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
                            workspace_id: Some(to_delete.workspace_id),
                        },
                    )
                    .await?;
            }
        }

        // TODO: Push notification trigger — notify all workspace clients
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

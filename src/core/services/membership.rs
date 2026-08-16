use std::{collections::HashMap, sync::Arc};

use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{Account, AccountDescribeParams},
            list::{ListResponse, RequestFilterParams},
            membership::{
                Membership, MembershipCreateParams, MembershipDeleteParams,
                MembershipDescribeParams, MembershipListParams, MembershipUpdateParams,
                MembershipWithPolicies,
            },
            permission::PermissionRule,
            role::{Role, RoleDescribeParams, RoleFilter, RoleListParams},
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            account::AccountService, validator::AuthValidator, permission::CANONICAL_PERMISSIONS,
            role::RoleService, workspace::WorkspaceService,
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
                MembershipRow, MembershipWithRoles,
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
        role_svc: Arc<RoleService<D, C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            acc_svc,
            role_svc,
            validator,
        }
    }

    pub fn set_cm(&mut self, cm: Arc<CacheManager<C>>) {
        self.cm = cm.clone()
    }

    async fn get_account(
        &self,
        ctx: &mut CoreCtx,
        id: Uuid,
        _workspace_id: Uuid,
    ) -> CoreResult<Account> {
        ctx.escalate_perms(&["account:describe"])?;

        let account = self
            .acc_svc
            .describe(
                ctx,
                AccountDescribeParams {
                    email: None,
                    id: Some(id),
                },
            )
            .await?;
        Ok(account)
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

    /// Creates a membership and optionally associates it with roles
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

        // Extract fields needed after params is consumed by into()
        let account_id = params.account_id;
        let workspace_id = params.workspace_id;
        let role_ids = params.role_ids.clone();
        let policy_ids = params.policy_ids.clone();

        let m_create: MembershipForCreate = params.into();

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
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DESCRIBE_PERMISSION])
            .await?;

        // Get Membership with Roles (Join query)
        let membership_with_roles: MembershipWithRoles =
            store.get_many_to_many(&store_ctx, &db_id).await?;

        // TODO: implement membership -> role -> permission join query
        let roles = self.get_roles(ctx, membership_with_roles.roles).await?;

        // Get Membership with Policies (Join query)
        let membership_with_policies = store
            .get_many_to_many_policies(&store_ctx, &db_id)
            .await?;
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

        // SELF-policy gate (US5/T037): a member mutating their OWN membership
        // (e.g. leaving the workspace) must hold the seeded self policy —
        // `membership:update` on `self` with the membership-account identity
        // constraint. Mutating another member's membership stays on the
        // permission-gated admin path and is NOT subject to this gate.
        if cur.account_id == ctx.account_id() {
            self.validator().validate_policy(
                ctx.policy_set(),
                "membership:update",
                "self",
                Some("membership.account.id === user.id"),
            )?;
        }

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

        // The membership no longer exists — drop its policy cache entry too.
        self.cm.policy.invalidate(res.id.into()).await?;

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
        cache::{
            entities::policy::PolicyCache, manager::CacheManager, mock::MockChx, traits::CacheEntity,
        },
        config::Config,
        core::{
            models::policy::{Policy, PolicySet},
            services::registry::ServiceRegistry,
        },
        store::{
            dbx::MockDbx,
            entities::{
                account::AccountRow,
                audit::AuditFields,
                id::DbId,
                membership::{
                    JoinedPolicyOnMembership, MembershipMeta, MembershipRow, MembershipScope,
                    MembershipStatus, MembershipWithPolicies, MembershipWithRoles,
                },
                policy::{PolicyEffect as StorePolicyEffect, PolicyMeta},
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

    fn membership_row(mem_id: Uuid, account_id: Uuid, ws_id: Uuid) -> MembershipRow {
        MembershipRow {
            id: mem_id.into(),
            account_id,
            workspace_id: ws_id,
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

    fn membership_with_policies(
        mem_id: Uuid,
        account_id: Uuid,
        ws_id: Uuid,
    ) -> MembershipWithPolicies {
        let row = membership_row(mem_id, account_id, ws_id);
        MembershipWithPolicies {
            id: row.id,
            membership: row,
            policies: vec![],
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
            // create -> describe -> get_many_to_many_policies (count)
            .with_one::<(i64,)>((0,))
            // create -> describe -> get_many_to_many_policies
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
                mem_id, account_id, ws_id,
            )))
            // create -> describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        let params = MembershipCreateParams {
            account_id,
            workspace_id: ws_id,
            role_ids: vec![],
            policy_ids: vec![],
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
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
            account_id,
            workspace_id: ws_id,
            role_ids: vec![],
            policy_ids: vec![],
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
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
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
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
        assert!(membership.policies.is_empty());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_describe_with_policies() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let joined_policy = |name: &str| JoinedPolicyOnMembership {
            id: DbId::from(Uuid::new_v4()),
            workspace_id: ws_id,
            name: Some(name.to_string()),
            effect: StorePolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint_expr: None,
            description: None,
            tags: vec![],
            meta: PolicyMeta::default(),
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: None,
        };

        let dbx = MockDbx::new()
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            .with_one::<(i64,)>((2,))
            .with_optional::<MembershipWithPolicies>(Some(MembershipWithPolicies {
                id: mem_id.into(),
                membership: membership_row(mem_id, account_id, ws_id),
                policies: vec![joined_policy("self-update"), joined_policy("deny-delete")],
            }))
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
        assert_eq!(
            membership.policies.len(),
            2,
            "describe should resolve membership policies"
        );
        assert!(
            membership
                .policies
                .iter()
                .any(|p| p.name.as_deref() == Some("self-update"))
        );

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
            // describe -> get_many_to_many_policies (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many_policies
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
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
            // describe -> get_many_to_many_policies (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many_policies
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
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
    async fn test_membership_update_invalidates_policy_cache() -> CoreResult<()> {
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
            // describe -> get_many_to_many_policies (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many_policies
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
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
            // describe -> get_many_to_many_policies (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many_policies
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
                mem_id, account_id, ws_id,
            )))
            // describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm.clone());

        // Seed the policy cache entry for the membership.
        let mut p = Policy::default();
        p.actions = vec!["membership:update".to_string()];
        p.resource = "self".to_string();
        let set = PolicySet::from_policies(vec![p]);
        cm.policy
            .write(&PolicyCache::new(mem_id, set), None)
            .await
            .unwrap();

        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute
        let updated = svc_reg
            .membership
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

        // The membership's policy cache entry must be gone.
        let fetched = cm.policy.fetch(&PolicyCache::new_key(mem_id)).await?;
        assert!(fetched.is_none(), "policy cache entry must be invalidated");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_update_self_gated_by_self_policy() -> CoreResult<()> {
        // (a) A member updating their OWN membership succeeds when the caller's
        // PolicySet grants the seeded self policy.
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> describe -> get_many_to_many (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // update -> describe -> get_many_to_many_policies (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
                mem_id, account_id, ws_id,
            )))
            // update -> describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)))
            // update -> store.update
            .with_optional::<MembershipRow>(Some(membership_row(mem_id, account_id, ws_id)))
            // update -> describe (final) -> get_many_to_many (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // update -> describe (final) -> get_many_to_many_policies (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
                mem_id, account_id, ws_id,
            )))
            // update -> describe (final) -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let svc = mock_svc(dbx);

        // Caller IS the target member, and holds the seeded self policy.
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.auth_cache.acc_id = account_id;
        let mut p = Policy::default();
        p.actions = vec!["membership:update".to_string()];
        p.resource = "self".to_string();
        p.constraint = Some("membership.account.id === user.id".to_string());
        ctx.set_policy_set(PolicySet::from_policies(vec![p]));

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

        assert_eq!(updated.id, mem_id);
        assert_eq!(updated.account_id, account_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_update_self_denied_without_self_policy() -> CoreResult<()> {
        // (b) A member updating their OWN membership is DENIED when the caller's
        // PolicySet lacks the seeded self policy.
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> describe -> get_many_to_many (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // update -> describe -> get_many_to_many_policies (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
                mem_id, account_id, ws_id,
            )))
            // update -> describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        // NOTE: no store.update / final describe mocks — the policy gate must
        // reject the mutation before any write happens.
        let svc = mock_svc(dbx);

        // Caller IS the target member, but holds no self policy (default-deny).
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.auth_cache.acc_id = account_id;
        ctx.set_policy_set(PolicySet::default());

        let err = svc
            .update(
                &mut ctx,
                MembershipUpdateParams {
                    id: mem_id,
                    workspace_id: ws_id,
                    tags: Some(vec!["veteran".to_string()]),
                    ..Default::default()
                },
            )
            .await;

        assert!(
            matches!(err, Err(CoreError::Auth(_))),
            "a self update without the self policy must be rejected with an auth error"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_update_non_self_not_gated_by_self_policy() -> CoreResult<()> {
        // (c) A non-self update (admin path) is NOT gated by the self policy:
        // it succeeds even when the caller's PolicySet is empty.
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> describe -> get_many_to_many (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // update -> describe -> get_many_to_many_policies (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
                mem_id, account_id, ws_id,
            )))
            // update -> describe -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)))
            // update -> store.update
            .with_optional::<MembershipRow>(Some(membership_row(mem_id, account_id, ws_id)))
            // update -> describe (final) -> get_many_to_many (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithRoles>(Some(membership_with_roles(
                mem_id, account_id, ws_id,
            )))
            // update -> describe (final) -> get_many_to_many_policies (count) + row
            .with_one::<(i64,)>((0,))
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
                mem_id, account_id, ws_id,
            )))
            // update -> describe (final) -> get_account
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let svc = mock_svc(dbx);

        // Caller is a DIFFERENT account with an empty (default-deny) policy set.
        let mut ctx = CoreCtx::bootstrap()?;
        assert_ne!(ctx.auth_cache.acc_id, account_id);
        ctx.set_policy_set(PolicySet::default());

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
            // describe -> get_many_to_many_policies (count)
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many_policies
            .with_optional::<MembershipWithPolicies>(Some(membership_with_policies(
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
}

use std::{collections::HashMap, sync::Arc};

use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{
        manager::CacheManager, stores::membership::MembershipCacheStore, traits::CacheExecutor,
    },
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{Account, AccountDescribeParams},
            list::{ListResponse, RequestFilterParams},
            membership::{
                Membership, MembershipCache, MembershipCreateParams, MembershipDeleteParams,
                MembershipDescribeParams, MembershipListParams, MembershipUpdateParams,
            },
            permission::PermissionRule,
            role::{Role, RoleDescribeParams, RoleFilter, RoleListParams},
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            account::AccountService, auth::AuthValidator, permission::CANONICAL_PERMISSIONS,
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
}

impl<D: DbExecutor, C: CacheExecutor> MembershipService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        acc_svc: Arc<AccountService<D, C>>,
        role_svc: Arc<RoleService<D, C>>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            acc_svc,
            role_svc,
        }
    }

    pub fn get_cached(&self) -> Option<MembershipCache> {
        let cache = self.cache();

        // TODO: implement get cached membership
        None
    }

    pub fn set_cm(&mut self, cm: Arc<CacheManager<C>>) {
        self.cm = cm.clone()
    }

    fn cache(&self) -> &MembershipCacheStore<C> {
        &self.cm.membership
    }

    async fn get_account(
        &self,
        ctx: &mut CoreCtx,
        id: Uuid,
        workspace_id: Uuid,
    ) -> CoreResult<Account> {
        ctx.extend_perms(&["account:describe"])?;

        let account = self
            .acc_svc
            .describe(
                ctx,
                AccountDescribeParams {
                    workspace_id,
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

        ctx.extend_perms(&["role:describe"]);

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

            for role in row.roles.iter() {
                let role = match roles_map.get(&role.id) {
                    Some(role) => role.clone(),
                    None => {
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
                        roles_map.insert(role.id, role.clone());
                        role
                    }
                };
                membership_roles.push(role);
            }

            let account = self
                .get_account(ctx, row.membership.account_id, row.membership.workspace_id)
                .await?;

            let membership = Membership::from((row.membership, membership_roles, account));

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

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        // Extract fields needed after params is consumed by into()
        let account_id = params.account_id;
        let workspace_id = params.workspace_id;
        let role_ids = params.role_ids.clone();

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

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        // Get Membership with Roles (Join query)
        let membership_with_roles: MembershipWithRoles =
            store.get_many_to_many(&store_ctx, &db_id).await?;

        let roles = self.get_roles(ctx, membership_with_roles.roles).await?;
        let account = self
            .get_account(
                ctx,
                membership_with_roles.membership.account_id,
                membership_with_roles.membership.workspace_id,
            )
            .await?;

        let membership = Membership::from((membership_with_roles.membership, roles, account));

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
        // TODO: this is the most naive way to listing membership,
        // it urgently needs dedicated store query

        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
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
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        let res = store
            .update(&store_ctx, &params.id.into(), params.into())
            .await?;

        // Invalidate the auth cache for the mutated membership so the next
        // request re-hydrates status/scope/version changes from the database.
        self.cm
            .invalidation
            .invalidate(res.id.into(), res.account_id, None)
            .await?;

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

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
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

        let res = store.delete(&store_ctx, &params.id.into()).await?;

        // Invalidate the auth cache for the deleted membership so no stale
        // cached auth data survives the deletion.
        self.cm
            .invalidation
            .invalidate(to_delete.id, to_delete.account.id, None)
            .await?;

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
    use std::mem;
    use std::sync::Arc;

    use super::*;
    use crate::{
        app::AppState,
        cache::{mock::MockChx, redis::RedisChx},
        config::Config,
        core::services::registry::ServiceRegistry,
        create_dbx_mock_unsafe,
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{
                account::AccountRow,
                credential::{CredentialForCreate, CredentialProvider},
                membership::{MembershipFilter, MembershipMeta, MembershipScope, MembershipStatus},
            },
            error::StoreError,
            meta::StoreId,
            stores::membership::MembershipStore,
            traits::{contains::FilterByContains, crud::*, join::GetOneToMany},
        },
    };
    use anyhow::Result;
    use modql::filter::{ListOptions, OpValsString};
    use serde_json::json;
    use serial_test::serial;
    use uuid::Uuid;

    async fn setup_membership_deps<D: DbExecutor, C: CacheExecutor>(
        app: &AppState<D, C>,
        ctx: &mut CoreCtx,
    ) -> CoreResult<(Uuid, Uuid)> {
        ctx.extend_perms(&[
            "workspace:create",
            "account:create",
            // "account:delete",
            "membership:create",
        ])?;

        // 1. Create a Workspace
        let ws = app
            .svc_reg
            .workspace
            .clone()
            .create(
                ctx,
                crate::core::models::workspace::WorkspaceCreateParams {
                    name: "Test WS".to_string(),
                    slug: format!("ws-{}", Uuid::new_v4()),
                    ..Default::default()
                },
            )
            .await?;

        // 2. Create an Account
        let acc = app
            .svc_reg
            .account
            .clone()
            .create(
                ctx,
                crate::core::models::account::AccountCreateParams {
                    workspace_id: ws.id,
                    email: format!("test-{}@example.com", Uuid::new_v4()),
                    name: "Member User".to_string(),
                    ..Default::default()
                },
            )
            .await?;

        Ok((ws.id, acc.id))
    }

    #[tokio::test]
    #[serial]
    async fn test_membership_crud_lifecycle() -> CoreResult<()> {
        let app = init_test().await;
        let mock_cache = Arc::new(MockChx::default());
        let cm = Arc::new(CacheManager::new(mock_cache));
        let svc_reg = ServiceRegistry::new(&app.config, app.sm.clone(), cm);
        let svc = svc_reg.membership.clone();
        let mut ctx = CoreCtx::new_root()?;

        // Setup Workspace and Account
        let (workspace_id, account_id) = setup_membership_deps(app, &mut ctx).await?;
        // let total_memberships = svc.list(&mut ctx,).await?.len();

        // --- 1. Create ---
        ctx.extend_perms(&["membership:create", "membership:describe"])?;
        let params = MembershipCreateParams {
            workspace_id,
            account_id,
            role_ids: vec![], // Logic currently doesn't assign these in service
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            tags: vec!["pioneer".to_string()],
            meta: MembershipMeta::default(),
        };

        let membership = svc.create(&mut ctx, params).await?;
        assert_eq!(membership.workspace_id, workspace_id);
        assert_eq!(membership.account.id, account_id);
        assert!(membership.tags.contains(&"pioneer".to_string()));

        // --- 2. Update ---
        ctx.extend_perms(&["membership:update"])?;
        let update_params = MembershipUpdateParams {
            id: membership.id,
            workspace_id,
            tags: Some(vec!["veteran".to_string()]),
            ..Default::default()
        };

        let updated = svc.update(&mut ctx, update_params).await?;
        assert!(updated.tags.contains(&"veteran".to_string()));

        // --- 3. Describe ---
        ctx.extend_perms(&["membership:describe"])?;
        let described = svc
            .describe(
                &mut ctx,
                MembershipDescribeParams {
                    id: membership.id,
                    workspace_id,
                },
            )
            .await?;
        assert_eq!(described.id, membership.id);

        // --- 4. List ---
        ctx.extend_perms(&["membership:list"])?;
        let list_params = MembershipListParams {
            workspace_id,
            filter: Some(RequestFilterParams {
                fields: None,
                tags: Some(vec!["veteran".to_string()]),
            }),
            ..Default::default()
        };
        let list = svc.list(&mut ctx, list_params).await?;

        println!("Membership List: {:#?}", list);
        assert_eq!(list.data.len(), 1);

        // --- 5. Delete ---
        ctx.extend_perms(&["membership:delete"])?;
        let deleted = svc
            .delete(
                &mut ctx,
                MembershipDeleteParams {
                    id: membership.id,
                    workspace_id,
                },
            )
            .await?;
        assert_eq!(deleted.id, membership.id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_mock_membership_success() -> CoreResult<()> {
        create_dbx_mock_unsafe!(
            MockDbxAccountRegister,
            fetch_one: {
                let acc = AccountRow::default();
                let result = unsafe { mem::transmute_copy::<AccountRow, O>(&acc) };
                mem::forget(acc);
                Ok(result)
            },
            fetch_optional: { Ok(None) },
            fetch_all: { Ok(vec![]) },
            execute: { Ok(1) }
        );

        let config = Config::test_config();

        // build store manager
        let dbx = Arc::new(MockDbxAccountRegister);
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with mock (no real Redis connection needed)
        let mock_cache = Arc::new(MockChx::default());
        let cm = Arc::new(CacheManager::new(mock_cache));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.membership.clone();

        Ok(())
    }
}

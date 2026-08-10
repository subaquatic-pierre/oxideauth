use std::{collections::HashMap, sync::Arc, todo};

use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::ListResponse,
            permission::PermissionRule,
            role::{
                Role, RoleCreateParams, RoleDeleteParams, RoleDescribeParams, RoleListParams,
                RoleUpdateParams,
            },
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            auth::AuthValidator, permission::CANONICAL_PERMISSIONS, permission::PermissionService,
            workspace::WorkspaceService,
        },
        traits::{
            list::RequestListParams,
            params::ValidateParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::{
        contains::FilterByContains,
        ctx::StoreCtx,
        entities::{
            id::DbId,
            role::{RoleForCreate, RoleRow, RoleWithPermissions},
        },
        join::{GetManyToMany, LinkManyToMany, ListManyToMany},
        manager::StoreManager,
        stores::role::RoleStore,
        traits::{crud::*, dbx::DbExecutor},
    },
};

pub struct RoleService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    perm_svc: Arc<PermissionService<D, C>>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for RoleService<D, C> {
    type CoreModel = Role;
    type ServiceStore = RoleStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.role
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }
}

impl<D: DbExecutor, C: CacheExecutor> RoleService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        perm_svc: Arc<PermissionService<D, C>>,
        cm: Arc<CacheManager<C>>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            perm_svc,
        }
    }

    // TODO: implement default roles for workspaces
    pub fn create_workspace_viewer_role(&self, ws_id: Uuid) -> CoreResult<Role> {
        todo!()
    }

    pub fn create_workspace_admin_role(&self, ws_id: Uuid) -> CoreResult<Role> {
        todo!()
    }

    /// Invalidates the account-level auth cache for every membership that carries
    /// the given role.
    ///
    /// A role mutation (e.g. deletion) changes the cached auth scope of all
    /// memberships holding that role. Each affected membership is invalidated
    /// individually (membership-scoped). Other memberships under the same
    /// account are unaffected — only the specific memberships holding the
    /// changed role lose their cached auth data.
    async fn invalidate_memberships_for_role(
        &self,
        store_ctx: &StoreCtx,
        role_id: Uuid,
    ) -> CoreResult<()> {
        let memberships = self
            .sm
            .membership
            .list_many_to_many(store_ctx, None, None, None)
            .await?;
        for membership in memberships {
            if membership.roles.iter().any(|r| Uuid::from(r.id) == role_id) {
                self.cm
                    .invalidation
                    .invalidate(membership.id.into(), membership.membership.account_id, None)
                    .await?;
            }
        }
        Ok(())
    }

    async fn hydrate_roles(
        &self,
        _ctx: &mut CoreCtx,
        rows: Vec<RoleWithPermissions>,
    ) -> CoreResult<Vec<Role>> {
        Ok(rows.into_iter().map(Role::from).collect())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for RoleService<D, C> {
    type CreateParams = RoleCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.create;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        let r_create = RoleForCreate {
            workspace_id: params.workspace_id,
            name: params.name,
            description: params.description,
            tags: params.tags,
            meta: params.meta,
        };

        let row = store.create(&store_ctx, r_create).await?;

        // Sync many-to-many permissions
        let perm_db_ids: Vec<DbId> = params
            .permission_ids
            .iter()
            .map(|id| DbId::from(*id))
            .collect();
        self.sm
            .role
            .set_many_to_many_links(&store_ctx, &row.id, perm_db_ids)
            .await?;

        self.describe(
            ctx,
            RoleDescribeParams {
                id: row.id.into(),
                workspace_id: params.workspace_id,
            },
        )
        .await
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for RoleService<D, C> {
    type DescribeParams = RoleDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, _workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let role_with_perms_row = store
            .get_many_to_many(&store_ctx, &params.id.into())
            .await?;
        let role = Role::from(role_with_perms_row);

        Ok(role)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for RoleService<D, C> {
    type ListParams = RoleListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>> {
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

        let data = self.hydrate_roles(ctx, data).await?;

        Ok(ListResponse::new(data, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for RoleService<D, C> {
    type UpdateParams = RoleUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.update;

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

        // TODO(T030): Push notification trigger — notify all workspace clients
        // that a role changed. Requires wiring a `ClientService` dependency
        // into `RoleService` (constructor + factory). Then call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "role_changed",
        //         serde_json::json!({ "role_id": res.id }),
        //         ctx,
        //     ).await;
        self.describe(
            ctx,
            RoleDescribeParams {
                id: res.id.into(),
                workspace_id: res.workspace_id.into(),
            },
        )
        .await
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for RoleService<D, C> {
    type DeleteParams = RoleDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.role.delete;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        // TODO: optimize delete operations across all services,
        // currently delete operation makes 2 database calls,
        // should only make one, may need to change the return type
        // of the delete method to only be deleted id
        let to_delete = self
            .describe(
                ctx,
                RoleDescribeParams {
                    id: params.id.into(),
                    workspace_id: params.workspace_id.into(),
                },
            )
            .await?;

        let _ = store.delete(&store_ctx, &to_delete.id.into()).await?;

        // Invalidate the auth cache for all memberships that carried the
        // deleted role, since their cached auth scope is now stale.
        self.invalidate_memberships_for_role(&store_ctx, to_delete.id)
            .await?;

        // TODO(T030): Push notification trigger — notify all workspace clients
        // that a role was deleted. Requires wiring a `ClientService` dependency
        // into `RoleService`. Then call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "role_changed",
        //         serde_json::json!({ "role_id": to_delete.id }),
        //         ctx,
        //     ).await;
        Ok(to_delete)
    }
}

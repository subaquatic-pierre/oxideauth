use std::{collections::HashMap, sync::Arc};

use uuid::Uuid;

use crate::{
    cache::{
        entities::auth::AuthCache,
        manager::CacheManager,
        traits::CacheExecutor,
    },
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::ListResponse,
            permission::PermissionCheck,
            role::{
                Role, RoleCreateParams, RoleDeleteParams, RoleDescribeParams, RoleListParams,
                RoleUpdateParams,
            },
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            auth::AuthValidator, permission::PermissionService, workspace::WorkspaceService,
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
        join::{GetManyToMany, ListManyToMany},
        manager::StoreManager,
        stores::role::RoleStore,
        traits::{crud::*, dbx::DbExecutor},
    },
};

pub struct RoleService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: WorkspaceService<D>,
    perm_svc: PermissionService<D, C>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D> for RoleService<D, C> {
    type CoreModel = Role;
    type ServiceStore = RoleStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.role
    }

    fn ws_svc(&self) -> &WorkspaceService<D> {
        &self.ws_svc
    }
}

impl<D: DbExecutor, C: CacheExecutor> RoleService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: WorkspaceService<D>,
        perm_svc: PermissionService<D, C>,
        cm: Arc<CacheManager<C>>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            perm_svc,
        }
    }

    /// Invalidates the auth cache for every membership that carries the given
    /// role. A role mutation (e.g. deletion) changes the cached auth scope of
    /// all memberships holding that role, so each of them must be re-hydrated.
    ///
    /// NOTE: naive reverse lookup — lists all memberships (with their roles)
    /// and filters in memory. Matches the existing patterns in the codebase.
    async fn invalidate_memberships_for_role(
        &self,
        store_ctx: &StoreCtx,
        role_id: Uuid,
    ) -> CoreResult<()> {
        let memberships = self
            .sm
            .membership
            .list_many_to_many(store_ctx, None, None)
            .await?;
        for membership in memberships {
            if membership
                .roles
                .iter()
                .any(|r| Uuid::from(r.id) == role_id)
            {
                let keyed = AuthCache::new_keyed(
                    membership.id.into(),
                    membership.membership.account_id,
                    None,
                );
                self.cm.auth.invalidate(&keyed).await?;
            }
        }
        Ok(())
    }

    async fn role_to_ws_map(
        &self,
        ctx: &mut CoreCtx,
        roles: Vec<RoleRow>,
    ) -> CoreResult<HashMap<Uuid, Workspace>> {
        let mut data = HashMap::new();
        let mut ws_map: HashMap<Uuid, Workspace> = HashMap::new();

        for role in roles.iter() {
            if let Some(ws) = ws_map.get(&role.workspace_id) {
                data.insert(role.id.into(), ws.clone());
            } else {
                let ws = self.get_workspace(ctx, role.workspace_id).await?;
                ws_map.insert(ws.id, ws.clone());
                data.insert(role.id.into(), ws);
            }
        }

        Ok(data)
    }

    async fn hydrate_roles(
        &self,
        ctx: &mut CoreCtx,
        rows: Vec<RoleWithPermissions>,
    ) -> CoreResult<Vec<Role>> {
        let mut workspaces: HashMap<Uuid, Workspace> = HashMap::new();

        let mut data: Vec<Role> = Vec::with_capacity(rows.len());

        // Hydrate results
        for row in rows.into_iter() {
            let workspace_id: Uuid = row.role.workspace_id;
            let workspace = match workspaces.get(&workspace_id) {
                Some(ws) => ws,
                None => {
                    let ws = self.get_workspace(ctx, workspace_id).await?;
                    let ws_id = ws.id;
                    workspaces.insert(ws_id, ws);
                    // SAFETY: can unwrap as insert occurs directly above
                    workspaces.get(&ws_id).unwrap()
                }
            };
            let role = Role::from_row_with_entities(row, workspace.clone())?;
            data.push(role);
        }

        Ok(data)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D> for RoleService<D, C> {
    type CreateParams = RoleCreateParams;
    const CREATE_PERMISSION: &'static str = "role:create";

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

        // TODO: Sync many-to-many permissions
        if !params.permission_ids.is_empty() {}

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

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D> for RoleService<D, C> {
    type DescribeParams = RoleDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = "role:describe";

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let role_with_perms_row = store
            .get_many_to_many(&store_ctx, &params.id.into())
            .await?;
        let ws = self
            .get_workspace(ctx, role_with_perms_row.role.workspace_id)
            .await?;
        let role = Role::from_row_with_entities(role_with_perms_row, ws)?;

        Ok(role)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D> for RoleService<D, C> {
    type ListParams = RoleListParams;
    const LIST_PERMISSION: &'static str = "role:list";

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

        if let Some(tags) = tags_filter.tags() {
            let data = store
                .filter_by_tags_contain(&store_ctx, tags.clone(), Some(options.clone()))
                .await?;
            let total = store.count_by_tags_contain(&store_ctx, tags).await?;

            // TODO: optimize filter Roles by tags with dedicated store SQL method
            let mut roles = vec![];
            for role in data {
                let role = self
                    .describe(
                        ctx,
                        RoleDescribeParams {
                            id: role.id.into(),
                            workspace_id: role.workspace_id.into(),
                        },
                    )
                    .await?;
                roles.push(role);
            }

            return Ok(ListResponse::new(roles, total, options));
        }

        if let Some(filter) = tags_filter.filter() {
            let filter = Some(filter);
            let data = store
                .list_many_to_many(&store_ctx, filter.clone(), Some(options.clone()))
                .await?;
            let total = store.count(&store_ctx, filter).await?;

            let data = self.hydrate_roles(ctx, data).await?;

            return Ok(ListResponse::new(data, total, options));
        }

        Ok(ListResponse::default())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D> for RoleService<D, C> {
    type UpdateParams = RoleUpdateParams;
    const UPDATE_PERMISSION: &'static str = "role:update";

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

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D> for RoleService<D, C> {
    type DeleteParams = RoleDeleteParams;
    const DELETE_PERMISSION: &'static str = "role:delete";

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

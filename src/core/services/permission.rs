use std::{
    collections::HashMap,
    sync::Arc,
};

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
            permission::{
                Permission, PermissionCheck, PermissionCreateParams, PermissionDeleteParams,
                PermissionDescribeParams, PermissionListParams, PermissionUpdateParams,
            },
            role::Role,
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{auth::AuthValidator, workspace::WorkspaceService},
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
        crud::{Create, Delete, Get, GetCount, List, Update},
        ctx::StoreCtx,
        entities::permission::PermissionRow,
        join::ListManyToMany,
        manager::StoreManager,
        stores::{permission::PermissionStore, role::RoleStore},
        traits::dbx::DbExecutor,
    },
};

pub struct PermissionService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: WorkspaceService<D>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D> for PermissionService<D, C> {
    type CoreModel = Permission;
    type ServiceStore = PermissionStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.permission
    }

    fn ws_svc(&self) -> &WorkspaceService<D> {
        &self.ws_svc
    }
}

impl<D: DbExecutor, C: CacheExecutor> PermissionService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: WorkspaceService<D>,
        cm: Arc<CacheManager<C>>,
    ) -> Self {
        Self { sm, cm, ws_svc }
    }

    /// Invalidates the account-level auth cache for every membership whose roles
    /// include the given permission.
    ///
    /// Changing a permission (its code/name or its deletion) affects the cached
    /// auth scope of every membership holding a role that grants it. Collecting
    /// distinct account IDs and invalidating per-account avoids redundant
    /// per-membership cache purge calls.
    async fn invalidate_memberships_for_permission(
        &self,
        store_ctx: &StoreCtx,
        permission_id: Uuid,
    ) -> CoreResult<()> {
        // Find the roles that include this permission.
        let roles = self
            .sm
            .role
            .list_many_to_many(store_ctx, None, None)
            .await?;
        let affected_role_ids: Vec<Uuid> = roles
            .iter()
            .filter(|r| {
                r.permissions
                    .iter()
                    .any(|p| Uuid::from(p.id) == permission_id)
            })
            .map(|r| Uuid::from(r.role.id))
            .collect();
        if affected_role_ids.is_empty() {
            return Ok(());
        }

        // Invalidate the auth cache for each affected membership individually.
        // This is membership-scoped: only memberships holding the changed
        // permission are invalidated. Other memberships under the same account
        // are unaffected, preserving acc_version across unrelated memberships.
        let memberships = self
            .sm
            .membership
            .list_many_to_many(store_ctx, None, None)
            .await?;
        for membership in memberships {
            if membership
                .roles
                .iter()
                .any(|r| affected_role_ids.contains(&Uuid::from(r.id)))
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

    async fn hydrate_permissions(
        &self,
        ctx: &mut CoreCtx,
        rows: Vec<PermissionRow>,
    ) -> CoreResult<Vec<Permission>> {
        let mut workspaces: HashMap<Uuid, Workspace> = HashMap::new();

        let mut perms: Vec<Permission> = Vec::with_capacity(rows.len());

        // // Hydrate results
        for row in rows.into_iter() {
            let workspace_id: Uuid = row.workspace_id;
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
            let perm = Permission::from_row_with_entities(row, workspace.clone())?;
            perms.push(perm);
        }

        Ok(perms)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D> for PermissionService<D, C> {
    type CreateParams = PermissionCreateParams;
    const CREATE_PERMISSION: &'static str = "permission:create";

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        // TODO: ensure cannot create same permission in same workspace
        // check database constraints

        let n_perm = store.create(&store_ctx, params.into()).await?;

        self.describe(
            ctx,
            PermissionDescribeParams {
                id: Some(n_perm.id.into()),
                workspace_id: n_perm.workspace_id.into(),
                code: None,
            },
        )
        .await
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D> for PermissionService<D, C> {
    type DescribeParams = PermissionDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = "permission:describe";

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let params = params.validate()?;
        let ws = self
            .ws_svc
            .describe(
                ctx,
                WorkspaceDescribeParams {
                    id: Some(params.workspace_id),
                    slug: None,
                },
            )
            .await?;

        if let Some(code) = params.code {
            let row = store
                .get_by_code(&store_ctx, &code, params.workspace_id.into())
                .await?;
            let perm = Permission::from_row_with_entities(row, ws)?;

            return Ok(perm);
        }

        if let Some(id) = params.id {
            let row = store.get(&store_ctx, &id.into()).await?;
            let perm = Permission::from_row_with_entities(row, ws)?;

            return Ok(perm);
        }

        Err(CoreError::InvalidParams(
            "Unable to get permission".to_string(),
        ))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D> for PermissionService<D, C> {
    type ListParams = PermissionListParams;
    const LIST_PERMISSION: &'static str = "permission:list";

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

        // filter by tags
        if let Some(tags) = tags_filter.tags() {
            let data = store
                .filter_by_tags_contain(&store_ctx, tags.clone(), Some(options.clone()))
                .await?;
            let total = store.count_by_tags_contain(&store_ctx, tags).await?;

            let perms = self.hydrate_permissions(ctx, data).await?;

            return Ok(ListResponse::new(perms, total, options));
        }

        // filter by filter
        if let Some(filter) = tags_filter.filter() {
            let filter = Some(filter);
            let data = store
                .list(&store_ctx, filter.clone(), Some(options.clone()))
                .await?;
            let total = store.count(&store_ctx, filter).await?;

            let perms = self.hydrate_permissions(ctx, data).await?;

            return Ok(ListResponse::new(perms, total, options));
        }

        Ok(ListResponse::default())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D> for PermissionService<D, C> {
    type UpdateParams = PermissionUpdateParams;
    const UPDATE_PERMISSION: &'static str = "permission:update";

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // TODO: ensure cannot update permission to an existing permission in same workspace
        // check database constraints
        let updated = store
            .update(&store_ctx, &params.id.into(), params.into())
            .await?;

        // Invalidate the auth cache for all memberships whose roles grant the
        // updated permission — its code/name may have changed.
        self.invalidate_memberships_for_permission(&store_ctx, updated.id.into())
            .await?;

        // TODO(T031): Push notification trigger — notify all workspace clients
        // that a permission changed. Requires wiring a `ClientService`
        // dependency into `PermissionService` (constructor + factory). Then
        // call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "permission_changed",
        //         serde_json::json!({ "permission_id": updated.id }),
        //         ctx,
        //     ).await;
        self.describe(
            ctx,
            PermissionDescribeParams {
                id: Some(updated.id.into()),
                workspace_id: updated.workspace_id,
                code: None,
            },
        )
        .await
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D> for PermissionService<D, C> {
    type DeleteParams = PermissionDeleteParams;
    const DELETE_PERMISSION: &'static str = "permission:delete";

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        // TODO: ensure cannot delete attached permission
        // check database constraints
        let to_delete = self
            .describe(
                ctx,
                PermissionDescribeParams {
                    id: Some(params.id.into()),
                    workspace_id: params.workspace_id,
                    code: None,
                },
            )
            .await?;

        let res = store.delete(&store_ctx, &to_delete.id.into()).await?;

        // Invalidate the auth cache for all memberships whose roles granted the
        // deleted permission — their cached auth scopes are now stale.
        self.invalidate_memberships_for_permission(&store_ctx, res.id.into())
            .await?;

        // TODO(T031): Push notification trigger — notify all workspace clients
        // that a permission was deleted. Requires wiring a `ClientService`
        // dependency into `PermissionService`. Then call:
        //     client_svc.push_to_workspace(
        //         params.workspace_id,
        //         "permission_changed",
        //         serde_json::json!({ "permission_id": to_delete.id }),
        //         ctx,
        //     ).await;
        Ok(to_delete)
    }
}

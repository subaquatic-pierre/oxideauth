use std::{collections::HashMap, sync::Arc};

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
            auth::AuthValidator,
            permission::{CANONICAL_PERMISSIONS, PermissionService},
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
        error::StoreError,
        join::{GetManyToMany, LinkManyToMany, ListManyToMany},
        manager::StoreManager,
        stores::{role::RoleStore, workspace::SYSTEM_CONST},
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

    pub async fn get_by_name(&self, ctx: &mut CoreCtx, name: &str) -> CoreResult<Role> {
        let store = self.store();
        let (store_ctx, _workspace) = self
            .scope_and_validate_ctx(ctx, ctx.scoped_ws_id(), &[Self::DESCRIBE_PERMISSION])
            .await?;

        let role = self
            .store()
            .get_by_name(&store_ctx, name, ctx.scoped_ws_id().into())
            .await?;

        let role = store.get_many_to_many(&store_ctx, &role.id).await?;
        let role = Role::from(role);

        Ok(role)
    }

    /// Creates the default "Workspace Viewer" role with the given permissions.
    pub async fn create_workspace_viewer_role(
        &self,
        ctx: &mut CoreCtx,
        ws_id: Uuid,
        perm_ids: Vec<Uuid>,
    ) -> CoreResult<Role> {
        let params = RoleCreateParams::new_workspace_system_role(
            ws_id,
            SYSTEM_CONST.workspace_viewer_role,
            Some("Default read-only workspace viewer role"),
            perm_ids,
        );
        self.create(ctx, params).await
    }

    /// Creates the default "Workspace Admin" role with the given permissions.
    pub async fn create_workspace_admin_role(
        &self,
        ctx: &mut CoreCtx,
        ws_id: Uuid,
        perm_ids: Vec<Uuid>,
    ) -> CoreResult<Role> {
        let params = RoleCreateParams::new_workspace_system_role(
            ws_id,
            SYSTEM_CONST.workspace_admin_role,
            Some("Default workspace administrator role"),
            perm_ids,
        );
        self.create(ctx, params).await
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
        // TODO: filter memberships by role_id
        let memberships = self
            .sm
            .membership
            .list_many_to_many(store_ctx, None, None, None)
            .await?;
        for membership in memberships {
            // TODO: invalidate auth cache
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

        // Extract permission_ids before params is consumed by into()
        let permission_ids = params.permission_ids.clone();
        let workspace_id = params.workspace_id;

        let r_create: RoleForCreate = params.into();

        let row = store.create(&store_ctx, r_create).await?;

        // Sync many-to-many permissions
        let perm_db_ids: Vec<DbId> = permission_ids.iter().map(|id| DbId::from(*id)).collect();
        self.sm
            .role
            .set_many_to_many_links(&store_ctx, &row.id, perm_db_ids)
            .await?;

        self.describe(
            ctx,
            RoleDescribeParams {
                id: row.id.into(),
                workspace_id,
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

        // TODO: invalidate auth cache
        self.invalidate_memberships_for_role(&store_ctx, res.id.into())
            .await?;

        // If this update ever syncs role->permission links (as create() does),
        // every membership holding this role would carry a stale
        // `auth_scope.permissions` and must be invalidated.
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

        // The `membership_role` join table references `role` with
        // `ON DELETE RESTRICT`, so Postgres rejects the delete with a foreign
        // key violation (SQLSTATE 23503) when the role is still assigned to
        // one or more memberships. Match on the error code and surface a
        // friendly message instead of leaking the raw constraint error.
        match store.delete(&store_ctx, &to_delete.id.into()).await {
            Ok(_) => {}
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::InvalidParams(format!(
                    "role '{}' is still assigned to one or more memberships and cannot be deleted",
                    to_delete.name
                )));
            }
            Err(err) => return Err(err.into()),
        }

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
                audit::AuditFields,
                id::DbId,
                membership::MembershipWithRoles,
                role::{RoleMeta, RoleWithPermissions},
                workspace::WorkspaceRow,
            },
        },
    };
    use serial_test::serial;
    use uuid::Uuid;

    /// Builds a `RoleRow` for the in-memory mock.
    fn role_row(id: Uuid, ws_id: Uuid, name: &str) -> RoleRow {
        RoleRow {
            id: id.into(),
            workspace_id: ws_id,
            name: name.to_string(),
            description: None,
            tags: vec![],
            meta: RoleMeta::default(),
            audit: AuditFields::default(),
        }
    }

    /// Builds a `RoleWithPermissions` (many-to-many joined row) for the mock.
    fn role_with_perms(id: Uuid, ws_id: Uuid, name: &str) -> RoleWithPermissions {
        RoleWithPermissions {
            id: id.into(),
            role: role_row(id, ws_id, name),
            permissions: vec![],
        }
    }

    /// Builds a `RoleService` backed by an in-memory `MockDbx` + `MockChx`.
    fn mock_svc(dbx: MockDbx) -> Arc<RoleService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.role.clone()
    }

    #[tokio::test]
    #[serial]
    async fn test_role_describe() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // get_many_to_many -> count_many guard
            .with_one::<(i64,)>((0,))
            // get_many_to_many -> fetch_optional joined row
            .with_optional::<RoleWithPermissions>(Some(role_with_perms(role_id, ws_id, "admin")));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["role:describe"])?;

        let role = svc
            .describe(
                &mut ctx,
                RoleDescribeParams {
                    id: role_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(role.id, role_id);
        assert_eq!(role.workspace_id, ws_id);
        assert_eq!(role.name, "admin");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_role_create() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // create -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // create -> store.create
            .with_one::<RoleRow>(role_row(role_id, ws_id, "dev"))
            // create -> set_many_to_many_links (delete existing join rows)
            .with_execute(Ok(0))
            // describe -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // describe -> get_many_to_many -> count_many guard
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many -> joined row
            .with_optional::<RoleWithPermissions>(Some(role_with_perms(role_id, ws_id, "dev")));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["role:create", "role:describe"])?;

        let params = RoleCreateParams {
            workspace_id: ws_id,
            name: "dev".to_string(),
            description: None,
            permission_ids: vec![],
            tags: vec![],
            meta: RoleMeta::default(),
        };

        let role = svc.create(&mut ctx, params).await?;

        assert_eq!(role.id, role_id);
        assert_eq!(role.name, "dev");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_role_create_workspace_viewer_role() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            .with_one::<RoleRow>(role_row(role_id, ws_id, SYSTEM_CONST.workspace_viewer_role))
            .with_execute(Ok(0))
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            .with_one::<(i64,)>((0,))
            .with_optional::<RoleWithPermissions>(Some(role_with_perms(
                role_id,
                ws_id,
                SYSTEM_CONST.workspace_viewer_role,
            )));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["role:create", "role:describe"])?;

        let role = svc
            .create_workspace_viewer_role(&mut ctx, ws_id, vec![])
            .await?;

        assert_eq!(role.id, role_id);
        assert_eq!(role.name, SYSTEM_CONST.workspace_viewer_role);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_role_list() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // list_many_to_many -> count guard
            .with_one::<(i64,)>((1,))
            // list_many_to_many -> joined rows
            .with_all::<RoleWithPermissions>(vec![role_with_perms(role_id, ws_id, "dev")])
            // count_with_tags_and_filter
            .with_one::<(i64,)>((1,));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["role:list"])?;

        let res = svc
            .list(
                &mut ctx,
                RoleListParams {
                    workspace_id: ws_id,
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id, role_id);
        assert_eq!(res.data[0].name, "dev");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_role_update() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // update -> store.update
            .with_optional::<RoleRow>(Some(role_row(role_id, ws_id, "renamed")))
            // invalidate_memberships_for_role -> list_many_to_many count guard
            .with_one::<(i64,)>((0,))
            // invalidate_memberships_for_role -> list_many_to_many rows (none)
            .with_all::<MembershipWithRoles>(vec![])
            // describe -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // describe -> get_many_to_many count_many guard
            .with_one::<(i64,)>((0,))
            // describe -> get_many_to_many joined row
            .with_optional::<RoleWithPermissions>(Some(role_with_perms(role_id, ws_id, "renamed")));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["role:update", "role:describe"])?;

        let params = RoleUpdateParams {
            id: role_id,
            workspace_id: ws_id,
            name: Some("renamed".to_string()),
            description: None,
            permission_ids: None,
            tags: None,
            meta: None,
        };

        let updated = svc.update(&mut ctx, params).await?;

        assert_eq!(updated.id, role_id);
        assert_eq!(updated.name, "renamed");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_role_delete() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // delete -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // delete -> describe -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // delete -> describe -> get_many_to_many count_many guard
            .with_one::<(i64,)>((0,))
            // delete -> describe -> get_many_to_many joined row
            .with_optional::<RoleWithPermissions>(Some(role_with_perms(role_id, ws_id, "dev")))
            // delete -> store.delete
            .with_optional::<RoleRow>(Some(role_row(role_id, ws_id, "dev")))
            // invalidate_memberships_for_role -> list_many_to_many count guard
            .with_one::<(i64,)>((0,))
            // invalidate_memberships_for_role -> list_many_to_many rows (none)
            .with_all::<MembershipWithRoles>(vec![]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["role:delete", "role:describe"])?;

        let deleted = svc
            .delete(
                &mut ctx,
                RoleDeleteParams {
                    id: role_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(deleted.id, role_id);
        assert_eq!(deleted.name, "dev");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_role_get_by_name() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // get_by_name -> list
            .with_all::<RoleRow>(vec![role_row(role_id, ws_id, "dev")])
            // get_many_to_many count_many guard
            .with_one::<(i64,)>((0,))
            // get_many_to_many joined row
            .with_optional::<RoleWithPermissions>(Some(role_with_perms(role_id, ws_id, "dev")));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["role:describe"])?;

        let role = svc.get_by_name(&mut ctx, "dev").await?;

        assert_eq!(role.id, role_id);
        assert_eq!(role.name, "dev");

        Ok(())
    }
}

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::ListResponse,
            permission::{
                Permission, PermissionCreateParams, PermissionDeleteParams,
                PermissionDescribeParams, PermissionFilter, PermissionListParams, PermissionRule,
                PermissionUpdateParams,
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
        crud::{Create, CreateMany, Delete, Get, GetCount, List, Update},
        ctx::StoreCtx,
        entities::{permission::PermissionRow, role::RoleFilter},
        error::StoreError,
        join::ListManyToMany,
        manager::StoreManager,
        stores::{permission::PermissionStore, role::RoleStore},
        traits::dbx::DbExecutor,
    },
};

pub struct PermissionService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for PermissionService<D, C> {
    type CoreModel = Permission;
    type ServiceStore = PermissionStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.permission
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }
}

impl<D: DbExecutor, C: CacheExecutor> PermissionService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        cm: Arc<CacheManager<C>>,
    ) -> Self {
        Self { sm, cm, ws_svc }
    }

    pub async fn create_many(
        &self,
        ctx: &mut CoreCtx,
        ws_id: Uuid,
        perms: Vec<PermissionCreateParams>,
    ) -> CoreResult<Vec<Permission>> {
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, ws_id, &[Self::CREATE_PERMISSION])
            .await?;
        let data = perms.into_iter().map(|el| el.into()).collect();
        let res = self.store().create_many(&store_ctx, data).await?;

        let ret = res.into_iter().map(|el| el.into()).collect();

        Ok(ret)
    }

    /// Invalidates the account-level auth cache for every membership whose roles
    /// include the given permission.
    async fn invalidate_memberships_for_permission(
        &self,
        store_ctx: &StoreCtx,
        permission_id: Uuid,
    ) -> CoreResult<()> {
        let roles = self
            .sm
            .role
            .list_containing_permissions(store_ctx, vec![permission_id.into()], None, None, None)
            .await?;
        if roles.is_empty() {
            return Ok(());
        }

        let role_ids = roles.iter().map(|el| el.id).collect();
        let memberships = self
            .sm
            .membership
            .list_containing_roles(store_ctx, role_ids, None, None, None)
            .await?;

        // TODO: implement bulk invalidate method
        for membership in memberships {
            self.cm.auth.invalidate(membership.id.into()).await?;
        }
        Ok(())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for PermissionService<D, C> {
    type CreateParams = PermissionCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.create;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        let n_perm = store.create(&store_ctx, params.into()).await?;

        Ok(n_perm.into())
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for PermissionService<D, C> {
    type DescribeParams = PermissionDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, _workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let params = params.validate()?;

        if let Some(id) = params.id {
            let row = store.get(&store_ctx, &id.into()).await?;
            let perm = Permission::from(row);

            return Ok(perm);
        }

        Err(CoreError::InvalidParams(
            "Unable to get permission".to_string(),
        ))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for PermissionService<D, C> {
    type ListParams = PermissionListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.list;

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

        // Combined query: tags (@> containment) + field filter
        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_with_tags_and_filter(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let perms = data.into_iter().map(Permission::from).collect();

        Ok(ListResponse::new(perms, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for PermissionService<D, C> {
    type UpdateParams = PermissionUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.update;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // The `permission_workspace_name_key` unique constraint on
        // (workspace_id, name) rejects the rename (SQLSTATE 23505) when another
        // permission in the same workspace already uses the target name. Match
        // on the typed error and surface a friendly message instead of leaking
        // the raw constraint error.
        let new_name = params.name.clone().unwrap_or_default();

        let updated = match store
            .update(&store_ctx, &params.id.into(), params.into())
            .await
        {
            Ok(updated) => updated,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::AlreadyExists(format!(
                    "a permission named '{new_name}' already exists in this workspace"
                )));
            }
            Err(err) => return Err(err.into()),
        };

        // Invalidate the auth cache for all memberships whose roles grant the
        // updated permission — its name may have changed.
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
            },
        )
        .await
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for PermissionService<D, C> {
    type DeleteParams = PermissionDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.permission.delete;

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
                PermissionDescribeParams {
                    id: Some(params.id.into()),
                    workspace_id: params.workspace_id,
                },
            )
            .await?;

        // The `role_permission` join table references `permission` with
        // `ON DELETE RESTRICT`, so Postgres rejects the delete (SQLSTATE 23503)
        // when the permission is still attached to one or more roles. Match on
        // the typed error and surface a friendly message instead of leaking the
        // raw constraint error.
        let res = match store.delete(&store_ctx, &to_delete.id.into()).await {
            Ok(res) => res,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::InvalidParams(format!(
                    "permission '{}' is still attached to one or more roles and cannot be deleted",
                    to_delete.name
                )));
            }
            Err(err) => return Err(err.into()),
        };

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
                audit::AuditFields, id::DbId, permission::PermissionMeta, role::RoleRow,
                workspace::WorkspaceRow,
            },
        },
    };
    use serial_test::serial;
    use uuid::Uuid;

    /// Builds a `PermissionRow` for the in-memory mock.
    fn perm_row(id: Uuid, ws_id: Uuid, name: &str) -> PermissionRow {
        PermissionRow {
            id: id.into(),
            workspace_id: ws_id,
            name: name.to_string(),
            description: None,
            tags: vec![],
            meta: PermissionMeta::default(),
            audit: AuditFields::default(),
        }
    }

    /// Builds a `PermissionService` backed by an in-memory `MockDbx` + `MockChx`.
    fn mock_svc(dbx: MockDbx) -> Arc<PermissionService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.permission.clone()
    }

    #[tokio::test]
    #[serial]
    async fn test_permission_create() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let perm_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // create -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // create -> store.create
            .with_one::<PermissionRow>(perm_row(perm_id, ws_id, "project:read"))
            // describe -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // describe -> store.get
            .with_optional::<PermissionRow>(Some(perm_row(perm_id, ws_id, "project:read")));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["permission:create", "permission:describe"])?;

        let params = PermissionCreateParams {
            workspace_id: ws_id,
            name: "project:read".to_string(),
            description: None,
            tags: vec![],
            meta: PermissionMeta::default(),
        };

        let perm = svc.create(&mut ctx, params).await?;

        assert_eq!(perm.id, perm_id);
        assert_eq!(perm.workspace_id, ws_id);
        assert_eq!(perm.name, "project:read");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_permission_create_many() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let perm_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // store.create_many
            .with_all::<PermissionRow>(vec![perm_row(perm_id, ws_id, "project:read")]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["permission:create"])?;

        let perms = vec![PermissionCreateParams {
            workspace_id: ws_id,
            name: "project:read".to_string(),
            description: None,
            tags: vec![],
            meta: PermissionMeta::default(),
        }];

        let created = svc.create_many(&mut ctx, ws_id, perms).await?;

        assert_eq!(created.len(), 1);
        assert_eq!(created[0].id, perm_id);
        assert_eq!(created[0].name, "project:read");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_permission_describe() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let perm_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // store.get
            .with_optional::<PermissionRow>(Some(perm_row(perm_id, ws_id, "project:read")));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["permission:describe"])?;

        let perm = svc
            .describe(
                &mut ctx,
                PermissionDescribeParams {
                    id: Some(perm_id),
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(perm.id, perm_id);
        assert_eq!(perm.name, "project:read");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_permission_describe_requires_id() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["permission:describe"])?;

        let res = svc
            .describe(
                &mut ctx,
                PermissionDescribeParams {
                    id: None,
                    workspace_id: ws_id,
                },
            )
            .await;

        assert!(
            matches!(res, Err(CoreError::InvalidParams(_))),
            "describe without an id must fail with InvalidParams"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_permission_list() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let perm_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // list_with_tags_and_filter
            .with_all::<PermissionRow>(vec![perm_row(perm_id, ws_id, "project:read")])
            // count_with_tags_and_filter
            .with_one::<(i64,)>((1,));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["permission:list"])?;

        let res = svc
            .list(
                &mut ctx,
                PermissionListParams {
                    workspace_id: ws_id,
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id, perm_id);
        assert_eq!(res.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_permission_update() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let perm_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // update -> store.update
            .with_optional::<PermissionRow>(Some(perm_row(perm_id, ws_id, "project:write")))
            // invalidate_memberships_for_permission -> no roles grant it
            .with_all::<RoleRow>(vec![])
            // describe -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // describe -> store.get
            .with_optional::<PermissionRow>(Some(perm_row(perm_id, ws_id, "project:write")));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["permission:update", "permission:describe"])?;

        let params = PermissionUpdateParams {
            id: perm_id,
            workspace_id: ws_id,
            name: Some("project:write".to_string()),
            description: None,
            tags: None,
            meta: None,
        };

        let updated = svc.update(&mut ctx, params).await?;

        assert_eq!(updated.id, perm_id);
        assert_eq!(updated.name, "project:write");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_permission_delete() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let perm_id = Uuid::new_v4();

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
            // delete -> describe -> store.get
            .with_optional::<PermissionRow>(Some(perm_row(perm_id, ws_id, "project:read")))
            // delete -> store.delete
            .with_optional::<PermissionRow>(Some(perm_row(perm_id, ws_id, "project:read")))
            // invalidate_memberships_for_permission -> no roles grant it
            .with_all::<RoleRow>(vec![]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["permission:delete", "permission:describe"])?;

        let deleted = svc
            .delete(
                &mut ctx,
                PermissionDeleteParams {
                    id: perm_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(deleted.id, perm_id);
        assert_eq!(deleted.name, "project:read");

        Ok(())
    }
}

// ============================================================================
// Per-domain permission structs
// ============================================================================

pub struct AccountPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl AccountPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new accounts"),
                (self.describe, "View account details"),
                (self.list, "List accounts"),
                (self.update, "Update account details"),
                (self.delete, "Delete accounts"),
            ]
        })
    }
}

pub struct WorkspacePermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl WorkspacePermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new workspaces"),
                (self.describe, "View workspace details"),
                (self.list, "List workspaces"),
                (self.update, "Update workspace settings"),
                (self.delete, "Delete workspaces"),
            ]
        })
    }
}

pub struct ProjectPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl ProjectPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new projects"),
                (self.describe, "View project details"),
                (self.list, "List projects"),
                (self.update, "Update project settings"),
                (self.delete, "Delete projects"),
            ]
        })
    }
}

pub struct MembershipPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl MembershipPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Invite members to workspace"),
                (self.describe, "View membership details"),
                (self.list, "List memberships"),
                (self.update, "Update membership roles"),
                (self.delete, "Remove members"),
            ]
        })
    }
}

pub struct RolePermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl RolePermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new roles"),
                (self.describe, "View role details"),
                (self.list, "List roles"),
                (self.update, "Update role permissions"),
                (self.delete, "Delete roles"),
            ]
        })
    }
}

pub struct ClientPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
    pub validate: &'static str,
    pub regenerate_secret: &'static str,
}

impl ClientPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 7]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Register new API clients"),
                (self.describe, "View client details"),
                (self.list, "List clients"),
                (self.update, "Update client configuration"),
                (self.delete, "Delete clients"),
                (self.validate, "Validate client credentials"),
                (self.regenerate_secret, "Regenerate client secret"),
            ]
        })
    }
}

pub struct CredentialPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl CredentialPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new credentials"),
                (self.describe, "View credential details"),
                (self.list, "List credentials"),
                (self.update, "Update credentials"),
                (self.delete, "Delete credentials"),
            ]
        })
    }
}

pub struct PermissionPermissions {
    pub create: &'static str,
    pub describe: &'static str,
    pub list: &'static str,
    pub update: &'static str,
    pub delete: &'static str,
}

impl PermissionPermissions {
    pub fn all(&self) -> &[(&'static str, &'static str)] {
        static ALL: OnceLock<[(&'static str, &'static str); 5]> = OnceLock::new();
        ALL.get_or_init(|| {
            [
                (self.create, "Create new permissions"),
                (self.describe, "View permission details"),
                (self.list, "List permissions"),
                (self.update, "Update permissions"),
                (self.delete, "Delete permissions"),
            ]
        })
    }
}

pub struct AuthPermissions {
    pub refresh: &'static str,
    pub revoke: &'static str,
}

impl AuthPermissions {
    pub fn all(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            (
                self.refresh,
                "Rotate authentication tokens (refresh access)",
            ),
            (
                self.revoke,
                "Revoke authentication tokens (invalidate sessions)",
            ),
        ]
    }
}

// ============================================================================
// Canonical permissions aggregate
// ============================================================================

pub struct CanonicalPermissions {
    pub account: AccountPermissions,
    pub workspace: WorkspacePermissions,
    pub project: ProjectPermissions,
    pub membership: MembershipPermissions,
    pub role: RolePermissions,
    pub client: ClientPermissions,
    pub credential: CredentialPermissions,
    pub permission: PermissionPermissions,
    pub auth: AuthPermissions,
}

impl CanonicalPermissions {
    /// Returns all canonical permissions as (name, description) tuples across all domains.
    pub fn all(&self) -> Vec<(&'static str, &'static str)> {
        let mut v = Vec::new();
        v.extend_from_slice(self.account.all());
        v.extend_from_slice(self.workspace.all());
        v.extend_from_slice(self.project.all());
        v.extend_from_slice(self.membership.all());
        v.extend_from_slice(self.role.all());
        v.extend_from_slice(self.client.all());
        v.extend_from_slice(self.credential.all());
        v.extend_from_slice(self.permission.all());
        v.extend_from_slice(&self.auth.all());
        v
    }

    pub fn default_workspace_viewer_perms(&self) -> Vec<&'static str> {
        let v = vec![
            // account
            CANONICAL_PERMISSIONS.account.describe, // describe own account
            CANONICAL_PERMISSIONS.account.update,   // update own account
            // workspace
            CANONICAL_PERMISSIONS.workspace.describe, // describe own workspace
            // project
            CANONICAL_PERMISSIONS.project.describe, // describe projects
            CANONICAL_PERMISSIONS.project.list,     // list projects
            // membership
            CANONICAL_PERMISSIONS.membership.describe, // describe memberships
            CANONICAL_PERMISSIONS.membership.list,     // list memberships
            // auth
            CANONICAL_PERMISSIONS.auth.refresh,
            CANONICAL_PERMISSIONS.auth.revoke,
        ];

        v
    }

    pub fn default_workspace_admin_perms(&self) -> Vec<&'static str> {
        let mut all = Vec::new();
        all.extend_from_slice(self.account.all());
        // all.extend_from_slice(self.workspace.all());
        all.extend_from_slice(self.project.all());
        all.extend_from_slice(self.membership.all());
        all.extend_from_slice(self.role.all());
        all.extend_from_slice(self.client.all());
        all.extend_from_slice(self.credential.all());
        all.extend_from_slice(self.permission.all());
        all.extend_from_slice(&self.auth.all());
        all.into_iter().map(|(name, _)| name).collect()
    }
}

pub const CANONICAL_PERMISSIONS: CanonicalPermissions = CanonicalPermissions {
    account: AccountPermissions {
        create: "account:create",
        describe: "account:describe",
        list: "account:list",
        update: "account:update",
        delete: "account:delete",
    },
    workspace: WorkspacePermissions {
        create: "workspace:create",
        describe: "workspace:describe",
        list: "workspace:list",
        update: "workspace:update",
        delete: "workspace:delete",
    },
    project: ProjectPermissions {
        create: "project:create",
        describe: "project:describe",
        list: "project:list",
        update: "project:update",
        delete: "project:delete",
    },
    membership: MembershipPermissions {
        create: "membership:create",
        describe: "membership:describe",
        list: "membership:list",
        update: "membership:update",
        delete: "membership:delete",
    },
    role: RolePermissions {
        create: "role:create",
        describe: "role:describe",
        list: "role:list",
        update: "role:update",
        delete: "role:delete",
    },
    client: ClientPermissions {
        create: "client:create",
        describe: "client:describe",
        list: "client:list",
        update: "client:update",
        delete: "client:delete",
        validate: "client:validate",
        regenerate_secret: "client:regenerateSecret",
    },
    credential: CredentialPermissions {
        create: "credential:create",
        describe: "credential:describe",
        list: "credential:list",
        update: "credential:update",
        delete: "credential:delete",
    },
    permission: PermissionPermissions {
        create: "permission:create",
        describe: "permission:describe",
        list: "permission:list",
        update: "permission:update",
        delete: "permission:delete",
    },
    auth: AuthPermissions {
        refresh: "auth:refresh",
        revoke: "auth:revoke",
    },
};

use std::{
    marker::PhantomData,
    sync::{Arc, OnceLock, Weak},
};

use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::traits::CacheExecutor,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::Account,
            list::ListResponse,
            permission::PermissionCreateParams,
            workspace::{
                Workspace, WorkspaceCreateParams, WorkspaceDeleteParams, WorkspaceDescribeParams,
                WorkspaceListParams, WorkspaceUpdateParams,
            },
        },
        services::{
            auth::AuthValidator,
            permission::{CANONICAL_PERMISSIONS, PermissionService},
            role::RoleService,
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
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::{
            account::{AccountFilter, AccountForCreate, AccountMeta},
            id::DbId,
            permission::{PermissionForCreate, PermissionMeta},
            workspace::{
                WorkspaceConfig as StoreWorkspaceConfig, WorkspaceFilter, WorkspaceForCreate,
                WorkspaceForUpdate,
            },
        },
        error::StoreError,
        manager::StoreManager,
        stores::workspace::WorkspaceStore,
        traits::{contains::FilterByContains, crud::*, dbx::DbExecutor},
        utils::ListOptionsValidator,
    },
};

pub struct WorkspaceService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,

    /// `Weak` to break the Arc cycle with `RoleService`.
    /// Set by `ServiceRegistry` via `wire_role_service()`.
    role_svc: OnceLock<Weak<RoleService<D, C>>>,

    /// `Weak` to break the Arc cycle with `PermissionService`.
    /// Set by `ServiceRegistry` via `wire_permission_service()`.
    perm_svc: OnceLock<Weak<PermissionService<D, C>>>,
}

impl<D: DbExecutor, C: CacheExecutor> WorkspaceService<D, C> {
    pub fn new(sm: Arc<StoreManager<D>>) -> Self {
        Self {
            sm,
            role_svc: OnceLock::new(),
            perm_svc: OnceLock::new(),
        }
    }

    /// --- Wiring (called once by ServiceRegistry) ---

    pub(crate) fn wire_role_service(&self, role: &Arc<RoleService<D, C>>) {
        self.role_svc
            .set(Arc::downgrade(role))
            .expect("wire_role_service must only be called once");
    }

    pub(crate) fn wire_permission_service(&self, perm: &Arc<PermissionService<D, C>>) {
        self.perm_svc
            .set(Arc::downgrade(perm))
            .expect("wire_permission_service must only be called once");
    }

    /// --- Resolvers ---

    fn role_svc(&self) -> Arc<RoleService<D, C>> {
        self.role_svc
            .get()
            .expect("RoleService not wired")
            .upgrade()
            .expect("RoleService Arc dropped before WorkspaceService")
    }

    fn perm_svc(&self) -> Arc<PermissionService<D, C>> {
        self.perm_svc
            .get()
            .expect("PermissionService not wired")
            .upgrade()
            .expect("PermissionService Arc dropped before WorkspaceService")
    }

    async fn get_workspace_id(
        &self,
        ctx: &CoreCtx,
        id: Option<Uuid>,
        slug: Option<String>,
    ) -> CoreResult<Uuid> {
        let store = self.store();

        let id: DbId = match (id, slug) {
            (Some(id), _) => id.into(),
            (None, Some(slug)) => match store.get_by_slug_opt(&ctx.into(), &slug).await? {
                Some(ws) => ws.id,
                None => {
                    return Err(CoreError::StoreError(StoreError::EntityNotFound {
                        entity: "workspace".to_string(),
                        id: slug.to_string(),
                    }));
                }
            },
            (None, None) => {
                return Err(CoreError::InvalidParams(
                    "Workspace ID or slug required".to_string(),
                ));
            }
        };

        Ok(id.into())
    }

    pub async fn get_workspace_by_slug_or_id(
        &self,
        ctx: &CoreCtx,
        slug_or_id: &str,
    ) -> CoreResult<Option<Workspace>> {
        let store = self.store();

        let ws = match Uuid::parse_str(&slug_or_id) {
            Ok(id) => store.get_opt(&ctx.into(), &id.into()).await?,
            Err(_) => store.get_by_slug_opt(&ctx.into(), &slug_or_id).await?,
        };

        Ok(ws.map(|el| el.into()))
    }

    /// Seeds all canonical permissions into a workspace (idempotent).
    ///
    /// Inserts every permission from `CANONICAL_PERMISSIONS.all()` into the
    /// given workspace.
    async fn populate_ws_perms(&self, ctx: &mut CoreCtx, workspace_id: Uuid) -> CoreResult<()> {
        let perm_svc = self.perm_svc();

        let perms: Vec<PermissionCreateParams> = CANONICAL_PERMISSIONS
            .all()
            .into_iter()
            .map(|(name, description)| {
                PermissionCreateParams::new_system(workspace_id, name, Some(description))
            })
            .collect();
        let created = perm_svc.create_many(ctx, workspace_id, perms).await?;

        tracing::info!(workspace_id = %workspace_id, "Canonical permissions seeded");
        Ok(())
    }

    /// Seeds default workspace roles (Viewer and Admin) with the appropriate
    /// canonical permissions.
    ///
    /// Must be called after `populate_ws_perms` so the permission rows exist.
    async fn populate_ws_roles(&self, ctx: &mut CoreCtx, workspace_id: Uuid) -> CoreResult<()> {
        let role_svc = self.role_svc();

        // 1. Collect permission name sets for each default role
        let viewer_names: Vec<String> = CANONICAL_PERMISSIONS
            .default_workspace_viewer_perms()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let admin_names: Vec<String> = CANONICAL_PERMISSIONS
            .default_workspace_admin_perms()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        // 2. Look up the just-seeded permission IDs (workspace-scoped via StoreCtx)

        let viewer_perms = self
            .sm
            .permission
            .find_all_many_by_names(&ctx.into(), viewer_names)
            .await?;
        let admin_perms = self
            .sm
            .permission
            .find_all_many_by_names(&ctx.into(), admin_names)
            .await?;

        let viewer_ids: Vec<Uuid> = viewer_perms.into_iter().map(|p| p.id.into()).collect();
        let admin_ids: Vec<Uuid> = admin_perms.into_iter().map(|p| p.id.into()).collect();

        // 3. Create both default roles
        role_svc
            .create_workspace_viewer_role(ctx, workspace_id, viewer_ids)
            .await?;
        role_svc
            .create_workspace_admin_role(ctx, workspace_id, admin_ids)
            .await?;

        tracing::info!(workspace_id = %workspace_id, "Default workspace roles seeded");
        Ok(())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for WorkspaceService<D, C> {
    type CoreModel = Workspace;

    type ServiceStore = WorkspaceStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.workspace
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        &self
    }

    fn should_remove_workspace_from_store_ctx(&self) -> bool {
        true
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for WorkspaceService<D, C> {
    type CreateParams = WorkspaceCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.workspace.create;

    /// Creates a new workspace.
    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceCreateParams,
    ) -> CoreResult<Workspace> {
        let store = self.store();

        let auth_validator = AuthValidator::new(ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::CREATE_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        // 1. Check if slug already exists
        if store
            .get_by_slug_opt(&store_ctx, &params.slug)
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists(
                "Workspace slug already exists".to_string(),
            ));
        }

        // 3. Execute store creation
        let new_workspace = store.create(&store_ctx, params.into()).await?;

        // 4. Seed canonical permissions into the new workspace
        let workspace_id: Uuid = new_workspace.id.into();
        ctx.extend_perms(&[CANONICAL_PERMISSIONS.permission.create]);
        self.populate_ws_perms(ctx, workspace_id).await?;

        // 5. Seed default roles (Viewer, Admin) into the new workspace
        ctx.extend_perms(&[
            CANONICAL_PERMISSIONS.role.create,
            CANONICAL_PERMISSIONS.role.describe,
        ]);
        self.populate_ws_roles(ctx, workspace_id).await?;

        Ok(new_workspace.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for WorkspaceService<D, C> {
    type DescribeParams = WorkspaceDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.workspace.describe;

    /// Retrieves a single workspace by ID or slug.
    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceDescribeParams,
    ) -> CoreResult<Workspace> {
        let store = self.store();

        let workspace_id = self.get_workspace_id(ctx, params.id, params.slug).await?;

        let auth_validator = AuthValidator::new(ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::DESCRIBE_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let res = store.get(&store_ctx, &workspace_id.into()).await?;

        Ok(res.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for WorkspaceService<D, C> {
    type ListParams = WorkspaceListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.workspace.list;

    /// Lists workspaces based on filter and options.
    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceListParams,
    ) -> CoreResult<ListResponse<Workspace>> {
        let store = self.store();
        let auth_validator = AuthValidator::new(ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::LIST_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

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

        let workspaces: Vec<Workspace> = data.into_iter().map(|el| el.into()).collect();
        Ok(ListResponse::new(workspaces, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for WorkspaceService<D, C> {
    type UpdateParams = WorkspaceUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.workspace.update;

    /// Updates an existing workspace.
    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceUpdateParams,
    ) -> CoreResult<Workspace> {
        let store = self.store();

        let auth_validator = AuthValidator::new(ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::UPDATE_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let id = self
            .get_workspace_id(ctx, params.id, params.slug.clone())
            .await?;

        let update_data: WorkspaceForUpdate = params.into();

        let res = store.update(&store_ctx, &id.into(), update_data).await?;

        Ok(res.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for WorkspaceService<D, C> {
    type DeleteParams = WorkspaceDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.workspace.delete;

    /// Deletes a workspace by ID or slug.
    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: WorkspaceDeleteParams,
    ) -> CoreResult<Workspace> {
        // Returns the ID of the deleted item
        let store = self.store();

        let auth_validator = AuthValidator::new(ctx);

        // validate permissions
        auth_validator.validate_ctx_perms(&[Self::DELETE_PERMISSION])?;

        // scope store_ctx
        let store_ctx = auth_validator.scope_store_workspace(None)?;

        let id = self.get_workspace_id(ctx, params.id, params.slug).await?;

        let deleted = store.delete(&store_ctx, &id.into()).await?;

        Ok(deleted.into())
    }
}

#[cfg(test)]
mod tests {
    use std::mem;
    use std::sync::Arc;

    use super::*;
    use crate::{
        core::models::list::RequestFilterParams,
        create_dbx_mock_unsafe,
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{
                account::AccountRow,
                credential::{CredentialForCreate, CredentialProvider},
            },
            error::StoreError,
            meta::StoreId,
            stores::workspace::{SYSTEM_CONST, WorkspaceStore},
            traits::{contains::FilterByContains, crud::*, join::GetOneToMany},
        },
    };
    use anyhow::Result;
    use modql::filter::{ListOptions, OpValsString};
    use serde_json::json;
    use serial_test::serial;
    use uuid::Uuid;

    #[tokio::test]
    #[serial]
    async fn test_workspace_describe() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_reg.workspace.clone();
        let mut ctx = CoreCtx::bootstrap()?;

        let system_ws = app
            .sm
            .workspace
            .get_by_slug_opt(&(&ctx).into(), SYSTEM_CONST.system_ws_slug)
            .await?
            .expect("system workspace not seeded");

        let mut params = WorkspaceDescribeParams::default();
        params.id = Some(system_ws.id.into());

        // NOTE: CoreCtx::bootstrap() grants `*:*`, so use a bare system context
        // (system workspace scope, no permissions) for the unauthorized check.
        let mut unauthorized_ctx = CoreCtx::system(Uuid::nil())?;
        let err = svc.describe(&mut unauthorized_ctx, params.clone()).await;

        ctx.extend_perms(&["workspace:describe"])?;

        let ok = svc.describe(&mut ctx, params).await;

        assert!(matches!(ok, Ok(..)));
        assert!(matches!(err, Err(..)));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_create() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_reg.workspace.clone();
        let mut ctx = CoreCtx::bootstrap()?;

        let slug = format!("test-ws-{}", Uuid::new_v4());
        let mut params = WorkspaceCreateParams::default();
        params.slug = slug.clone();
        params.name = "Test Workspace".to_string();

        // 1. Test unauthorized (missing permission)
        // NOTE: CoreCtx::bootstrap() grants `*:*`, so use a bare system context
        // (system workspace scope, no permissions) for the unauthorized check.
        let mut unauthorized_ctx = CoreCtx::system(Uuid::nil())?;
        let err = svc.create(&mut unauthorized_ctx, params.clone()).await;
        assert!(
            err.is_err(),
            "Should fail without workspace:create permission"
        );

        // 2. Test success
        ctx.extend_perms(&["workspace:create"])?;
        let workspace = svc.create(&mut ctx, params.clone()).await?;

        assert_eq!(workspace.name, "Test Workspace");
        assert_eq!(workspace.slug, slug);

        // 3. Test duplicate slug
        let err_dup = svc.create(&mut ctx, params).await;
        assert!(matches!(err_dup, Err(CoreError::AlreadyExists(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_list() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_reg.workspace.clone();
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["workspace:create"])?;

        let mut params = WorkspaceCreateParams::default();
        params.name = "list-ns-a".to_string();
        params.slug = "list-ns-a-test-a".to_string();
        params.description = Some("LIST_BY_DESCRIPTION".to_string());
        let workspace = svc.create(&mut ctx, params.clone()).await?;
        params.name = "list-ns-b".to_string();
        params.slug = "list-ns-a-test-b".to_string();
        let workspace = svc.create(&mut ctx, params.clone()).await?;

        ctx.extend_perms(&["workspace:list"])?;

        let filter: WorkspaceFilter = json!({ "description": "LIST_BY_DESCRIPTION" }).try_into()?;
        let filter = RequestFilterParams {
            fields: Some(filter),
            tags: None,
        };
        let options = ListOptions::from_limit(1);

        let params = WorkspaceListParams {
            filter: Some(filter),
            options: Some(options),
        };

        let res = svc.list(&mut ctx, params).await?;

        assert!(res.data.len() == 1);
        assert!(res.metadata.total >= 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_update() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_reg.workspace.clone();
        let mut ctx = CoreCtx::bootstrap()?;

        // Setup: Create a workspace to update
        ctx.extend_perms(&["workspace:update", "workspace:create"])?;
        let slug = format!("update-me-{}", Uuid::new_v4());
        let created = svc
            .create(
                &mut ctx,
                WorkspaceCreateParams {
                    name: "Original Name".to_string(),
                    slug,
                    ..Default::default()
                },
            )
            .await?;

        // Update name
        let update_params = WorkspaceUpdateParams {
            id: Some(created.id),
            name: Some("Updated Name".to_string()),
            ..Default::default()
        };

        let updated = svc.update(&mut ctx, update_params).await?;
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.id, created.id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_delete() -> CoreResult<()> {
        let app = init_test().await;
        let svc = app.svc_reg.workspace.clone();
        let mut ctx = CoreCtx::bootstrap()?;

        ctx.extend_perms(&["workspace:create", "workspace:delete", "workspace:describe"])?;

        // Setup: Create workspace
        let slug = format!("delete-me-{}", Uuid::new_v4());
        let created = svc
            .create(
                &mut ctx,
                WorkspaceCreateParams {
                    name: "To Be Deleted".to_string(),
                    slug: slug.clone(),
                    ..Default::default()
                },
            )
            .await?;

        // Delete
        let delete_params = WorkspaceDeleteParams {
            id: Some(created.id),
            slug: None,
        };
        let deleted = svc.delete(&mut ctx, delete_params).await?;
        assert_eq!(deleted.id, created.id);

        // Verify it's gone
        let desc_params = WorkspaceDescribeParams {
            id: Some(created.id),
            slug: None,
        };
        let err = svc.describe(&mut ctx, desc_params).await;
        assert!(err.is_err());

        Ok(())
    }
}

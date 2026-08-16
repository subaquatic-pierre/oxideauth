use std::{
    marker::PhantomData,
    sync::{Arc, OnceLock, Weak},
};

use modql::filter::ListOptions;
use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{entities::workspace::WorkspaceCache, manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::Account,
            list::ListResponse,
            membership::MembershipFilter,
            permission::{PermissionCreateManyParams, PermissionCreateParams},
            role::WorkspaceRoleCreateParams,
            workspace::{
                Workspace, WorkspaceCreateParams, WorkspaceDeleteParams, WorkspaceDescribeParams,
                WorkspaceListParams, WorkspaceUpdateParams,
            },
        },
        services::{
            validator::AuthValidator,
            permission::{CANONICAL_PERMISSIONS, PermissionService},
            project::ProjectService,
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
            project::ProjectForCreate,
            workspace::{
                WorkspaceConfig as StoreWorkspaceConfig, WorkspaceFilter, WorkspaceForCreate,
                WorkspaceForUpdate,
            },
        },
        error::StoreError,
        manager::StoreManager,
        stores::workspace::WorkspaceStore,
        traits::{contains::FilterByContains, crud::*, dbx::DbExecutor},
        utils::{LIST_LIMIT_DEFAULT, ListOptionsValidator},
    },
};

pub struct WorkspaceService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    validator: Arc<AuthValidator>,

    /// `Weak` to break the Arc cycle with `RoleService`.
    /// Set by `ServiceRegistry` via `wire_role_service()`.
    role_svc: OnceLock<Weak<RoleService<D, C>>>,

    /// `Weak` to break the Arc cycle with `PermissionService`.
    /// Set by `ServiceRegistry` via `wire_permission_service()`.
    perm_svc: OnceLock<Weak<PermissionService<D, C>>>,

    /// `Weak` to break the Arc cycle with `ProjectService`.
    /// Set by `ServiceRegistry` via `wire_project_service()`.
    project_svc: OnceLock<Weak<ProjectService<D, C>>>,
}

impl<D: DbExecutor, C: CacheExecutor> WorkspaceService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            cm,
            validator,
            role_svc: OnceLock::new(),
            perm_svc: OnceLock::new(),
            project_svc: OnceLock::new(),
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

    pub(crate) fn wire_project_service(&self, project: &Arc<ProjectService<D, C>>) {
        self.project_svc
            .set(Arc::downgrade(project))
            .expect("wire_project_service must only be called once");
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

    fn project_svc(&self) -> Arc<ProjectService<D, C>> {
        self.project_svc
            .get()
            .expect("ProjectService not wired")
            .upgrade()
            .expect("ProjectService Arc dropped before WorkspaceService")
    }

    pub async fn get_and_cache(
        &self,
        ctx: &CoreCtx,
        params: &WorkspaceDescribeParams,
    ) -> CoreResult<Workspace> {
        let ws = self.get_workspace_by_slug_or_id(ctx, params).await?;
        self.cm.workspace.write(&ws.clone().into(), None).await?;

        Ok(ws)
    }

    /// Resolves a workspace from a typed id-or-slug descriptor.
    ///
    /// When `params.id` is present the workspace is fetched by id, otherwise
    /// `params.slug` is used for a slug lookup.
    pub async fn get_workspace_by_slug_or_id(
        &self,
        ctx: &CoreCtx,
        params: &WorkspaceDescribeParams,
    ) -> CoreResult<Workspace> {
        let store = self.store();

        // The `workspace` table has no `workspace_id` column, so this lookup
        // must run unscoped.
        // NOTE(workspace-scope): unscoped — the workspace table has no workspace_id column.
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);

        let slug_or_id = params.id_or_slug()?;

        let ws = match Uuid::parse_str(&slug_or_id) {
            Ok(id) => store.get_opt(&store_ctx, &id.into()).await?,
            Err(_) => store.get_by_slug_opt(&store_ctx, &slug_or_id).await?,
        };

        let ws = ws.ok_or(CoreError::NotFound(format!(
            "Unable to get workspace with identifier: {}",
            slug_or_id
        )))?;

        Ok(ws.into())
    }

    /// Seeds all canonical permissions into a workspace (idempotent).
    ///
    /// Inserts every permission from `CANONICAL_PERMISSIONS.all()` into the
    /// given workspace.
    async fn populate_ws_perms(&self, ctx: &mut CoreCtx, workspace_id: Uuid) -> CoreResult<()> {
        let perm_svc: Arc<PermissionService<D, C>> = self.perm_svc();

        let perms: Vec<PermissionCreateParams> = CANONICAL_PERMISSIONS
            .all()
            .into_iter()
            .map(|(name, description)| {
                PermissionCreateParams::new_system(workspace_id, name, Some(description))
            })
            .collect();
        let created = perm_svc
            .create_many(
                ctx,
                PermissionCreateManyParams {
                    workspace_id,
                    permissions: perms,
                },
            )
            .await?;

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
        // The admin role carries the single system-wide wildcard permission.
        let admin_names: Vec<String> = vec!["*:*".to_string()];

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
            .create_workspace_viewer_role(
                ctx,
                WorkspaceRoleCreateParams {
                    workspace_id,
                    permission_ids: viewer_ids,
                },
            )
            .await?;
        role_svc
            .create_workspace_admin_role(
                ctx,
                WorkspaceRoleCreateParams {
                    workspace_id,
                    permission_ids: admin_ids,
                },
            )
            .await?;

        tracing::info!(workspace_id = %workspace_id, "Default workspace roles seeded");
        Ok(())
    }

    async fn invalidate_all_ws_autch_cache(
        &self,
        ctx: &mut CoreCtx,
        ws: &Workspace,
    ) -> CoreResult<()> {
        let ws_id = ws.id;
        // NOTE(workspace-scope): unscoped — the workspace table has no workspace_id column.
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);

        // TODO: there may be more that 500 memberships in the workspace
        // which means we need to coninue

        let mem_filter: MembershipFilter = json!({"workspace_id":&ws_id}).try_into()?;

        let total = self
            .sm
            .membership
            .count(&store_ctx, Some(mem_filter.clone()))
            .await?;

        let mut cur_offset = 0;
        let mut mem_ids: Vec<Uuid> = vec![];

        while (cur_offset < total) {
            let opts = ListOptions {
                limit: Some(LIST_LIMIT_DEFAULT),
                offset: Some(cur_offset),
                order_bys: Some("id".into()),
            };

            let mems = self
                .sm
                .membership
                .list(&store_ctx, Some(mem_filter.clone()), Some(opts.clone()))
                .await?;

            let ids: Vec<Uuid> = mems.into_iter().map(|el| el.id.into()).collect();

            mem_ids.extend_from_slice(&ids);

            // increment offset
            cur_offset += LIST_LIMIT_DEFAULT;
        }

        for id in mem_ids {
            self.cm.auth.invalidate(id).await?;
        }

        Ok(())
    }

    /// Creates a default project in the new workspace, owned by the
    /// workspace creator. Called after permissions and roles are seeded.
    async fn populate_default_project(
        &self,
        ctx: &mut CoreCtx,
        workspace_id: Uuid,
    ) -> CoreResult<()> {
        let store_ctx: StoreCtx = ctx.into();

        let project_for_create = ProjectForCreate {
            workspace_id,
            name: "Default".to_string(),
            code: None,
            description: Some("Default project for workspace".to_string()),
            owner: ctx.account_id().into(),
            config: Default::default(),
            tags: vec![],
            meta: Default::default(),
        };

        self.sm
            .project
            .create(&store_ctx, project_for_create)
            .await?;

        tracing::info!(workspace_id = %workspace_id, "Default project created");
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

    fn validator(&self) -> &AuthValidator {
        self.validator.as_ref()
    }

    fn is_scoped(&self) -> bool {
        false
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

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::CREATE_PERMISSION]).await?;

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
        let new_workspace = store
            .create(&store_ctx, params.into_store_params(ctx.account_id()))
            .await?;

        ctx.set_scoped_ws(new_workspace.clone().into());

        // 4. Seed canonical permissions into the new workspace
        let workspace_id: Uuid = new_workspace.id.clone().into();
        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.permission.create]);
        self.populate_ws_perms(ctx, workspace_id).await?;

        // 5. Seed default roles (Viewer, Admin) into the new workspace
        ctx.escalate_perms(&[
            CANONICAL_PERMISSIONS.role.create,
            CANONICAL_PERMISSIONS.role.describe,
        ]);
        self.populate_ws_roles(ctx, workspace_id).await?;

        // 6. Create default project in the new workspace
        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.project.create]);
        self.populate_default_project(ctx, workspace_id).await?;

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

        let workspace = self
            .get_workspace_by_slug_or_id(ctx, &params)
            .await?;

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::DESCRIBE_PERMISSION]).await?;

        let res = store.get(&store_ctx, &workspace.id.into()).await?;

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
        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::LIST_PERMISSION]).await?;

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

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::UPDATE_PERMISSION]).await?;

        let ws = self
            .get_workspace_by_slug_or_id(
                ctx,
                &WorkspaceDescribeParams {
                    id: params.id,
                    slug: params.slug.clone(),
                },
            )
            .await?;

        let update_data: WorkspaceForUpdate = params.clone().into();

        let res = store.update(&store_ctx, &ws.id.into(), update_data).await?;

        let ws: Workspace = res.into();

        self.cm.workspace.invalidate(ws.id).await?;
        // self.invalidate_all_ws_autch_cache(ctx, &ws).await?;

        Ok(ws)
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

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self.scope_and_validate(ctx, None, &[Self::DELETE_PERMISSION]).await?;

        let ws = self
            .get_workspace_by_slug_or_id(
                ctx,
                &WorkspaceDescribeParams {
                    id: params.id,
                    slug: params.slug.clone(),
                },
            )
            .await?;

        let deleted = store.delete(&store_ctx, &ws.id.into()).await?;
        self.cm.workspace.invalidate(deleted.id.into()).await?;

        Ok(deleted.into())
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
        store::{dbx::MockDbx, entities::workspace::WorkspaceRow},
    };
    use serial_test::serial;
    use uuid::Uuid;

    /// Builds a `WorkspaceService` backed by an in-memory `MockDbx` + `MockChx`
    /// so tests exercise the service logic without a real database or Redis.
    fn mock_svc(dbx: MockDbx) -> Arc<WorkspaceService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.workspace.clone()
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_describe() -> CoreResult<()> {
        let dbx = MockDbx::new()
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow::default())) // get_workspace_by_slug_or_id
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow::default())); // store.get
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;

        let mut params = WorkspaceDescribeParams::default();
        params.id = Some(Uuid::new_v4());

        let ok = svc.describe(&mut ctx, params).await;

        assert!(matches!(ok, Ok(..)));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_create_duplicate_slug() -> CoreResult<()> {
        // -- Setup
        let dbx = MockDbx::new()
            // get_by_slug_opt -> list (a workspace with this slug already exists)
            .with_all::<WorkspaceRow>(vec![WorkspaceRow {
                slug: "existing-slug".to_string(),
                ..Default::default()
            }]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["workspace:create"])?;

        let mut params = WorkspaceCreateParams::default();
        params.slug = "existing-slug".to_string();
        params.name = "Test Workspace".to_string();

        // -- Execute
        let err = svc.create(&mut ctx, params).await;

        // -- Assert
        assert!(
            matches!(err, Err(CoreError::AlreadyExists(_))),
            "creating a workspace with an existing slug must fail with AlreadyExists"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_list() -> CoreResult<()> {
        let dbx = MockDbx::new()
            .with_all::<WorkspaceRow>(vec![WorkspaceRow {
                name: "list-ns-a".to_string(),
                ..Default::default()
            }])
            .with_one::<(i64,)>((1,));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["workspace:list"])?;

        let res = svc.list(&mut ctx, WorkspaceListParams::default()).await?;

        assert!(res.data.len() == 1);
        assert!(res.metadata.total >= 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_update() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let ws = WorkspaceRow {
            id: ws_id.into(),
            name: "Original Name".to_string(),
            ..Default::default()
        };
        let ws_updated = WorkspaceRow {
            id: ws_id.into(),
            name: "Updated Name".to_string(),
            ..Default::default()
        };

        let dbx = MockDbx::new()
            .with_optional::<WorkspaceRow>(Some(ws.clone())) // get_workspace_by_slug_or_id
            .with_optional::<WorkspaceRow>(Some(ws_updated.clone())) // store.update
            .with_all::<WorkspaceRow>(vec![ws.clone()]); // store.update
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.set_scoped_ws(ws.into());
        ctx.escalate_perms(&["workspace:update"])?;

        let update_params = WorkspaceUpdateParams {
            id: Some(ws_id),
            name: Some("Updated Name".to_string()),
            ..Default::default()
        };

        let updated = svc.update(&mut ctx, update_params).await?;
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_delete() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let dbx = MockDbx::new()
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            })) // get_workspace_by_slug_or_id
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            })) // store.delete
            .with_optional::<WorkspaceRow>(None); // describe after delete -> NotFound
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["workspace:delete", "workspace:describe"])?;

        let delete_params = WorkspaceDeleteParams {
            id: Some(ws_id),
            ..Default::default()
        };
        let deleted = svc.delete(&mut ctx, delete_params).await?;
        assert_eq!(deleted.id, ws_id);

        // Verify it's gone
        let desc_params = WorkspaceDescribeParams {
            id: Some(ws_id),
            ..Default::default()
        };
        let err = svc.describe(&mut ctx, desc_params).await;
        assert!(err.is_err());

        Ok(())
    }
}

use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

use crate::{
    cache::{entities::workspace::WorkspaceCache, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::{ListResponse, RequestFilterParams},
            permission::{PermissionEngine, PermissionRule},
            project::{
                Project, ProjectCreateParams, ProjectDeleteParams, ProjectDescribeParams,
                ProjectFilter, ProjectListParams, ProjectUpdateParams,
            },
            workspace::{Workspace, WorkspaceDescribeParams},
        },
        services::{
            permission::CANONICAL_PERMISSIONS, validator::AuthValidator,
            workspace::WorkspaceService,
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
        entities::{
            id::DbId,
            project::{ProjectForCreate, ProjectForUpdate, ProjectRow},
        },
        error::StoreError,
        manager::StoreManager,
        stores::project::ProjectStore,
        traits::{contains::FilterByContains, crud::*, dbx::DbExecutor},
        utils::ListOptionsValidator,
    },
};

pub struct ProjectService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for ProjectService<D, C> {
    type CoreModel = Project;

    type ServiceStore = ProjectStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.project
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }
}

impl<D: DbExecutor, C: CacheExecutor> ProjectService<D, C> {
    pub fn new(sm: Arc<StoreManager<D>>, ws_svc: Arc<WorkspaceService<D, C>>) -> Self {
        Self { sm, ws_svc }
    }

    // async fn hydrate_projects(
    //     &self,
    //     ctx: &mut CoreCtx,
    //     rows: Vec<ProjectRow>,
    // ) -> CoreResult<Vec<Project>> {
    //     let mut workspaces: HashMap<Uuid, WorkspaceCache> = HashMap::new();

    //     let mut projects: Vec<Project> = Vec::with_capacity(rows.len());

    //     // // Hydrate results
    //     for row in rows.into_iter() {
    //         let workspace_id: Uuid = row.workspace_id;
    //         let workspace = match workspaces.get(&workspace_id) {
    //             Some(ws) => ws,
    //             None => {
    //                 let ws = self.get_workspace(ctx, workspace_id).await?;
    //                 let ws_id = ws.id;
    //                 workspaces.insert(ws_id, ws);
    //                 // SAFETY: can unwrap as insert occurs directly above
    //                 workspaces.get(&ws_id).unwrap()
    //             }
    //         };
    //         let project = Project::from_row_with_workspace(row, workspace.clone())?;
    //         projects.push(project);
    //     }

    //     Ok(projects)
    // }

    /// Resolves a Project's DbId from either Uuid or code, enforcing workspace scoping.
    // async fn get_project_id(
    //     &self,
    //     store_ctx: &StoreCtx,
    //     id: Option<Uuid>,
    //     code: Option<String>,
    //     workspace_id: Uuid,
    // ) -> CoreResult<DbId> {
    //     let store = self.store();

    //     let id_db: DbId = match (id, code) {
    //         (Some(id), _) => id.into(), // If UUID provided, use it directly (store handles scope)
    //         (None, Some(code)) => {
    //             // If code provided, lookup by code and ensure it's in the correct workspace
    //             match store
    //                 .get_by_code(&store_ctx, &code, &workspace_id.into())
    //                 .await?
    //             {
    //                 Some(project_row) => project_row.id,
    //                 None => {
    //                     return Err(CoreError::StoreError(StoreError::EntityNotFound {
    //                         entity: "project".to_string(),
    //                         id: format!("code:'{}' in ws:'{}'", code, workspace_id),
    //                     }));
    //                 }
    //             }
    //         }
    //         (None, None) => {
    //             return Err(CoreError::InvalidParams(
    //                 "Project ID or code required for operation".to_string(),
    //             ));
    //         }
    //     };

    //     Ok(id_db)
    // }

    async fn get_by_id_or_code(&self, ctx: &mut CoreCtx, id_or_code: &str) -> CoreResult<Project> {
        let store = self.store();
        let store_ctx = ctx.into();

        let project: ProjectRow = match Uuid::parse_str(&id_or_code) {
            Ok(id) => store.get(&store_ctx, &id.into()).await?,
            Err(_) => {
                store
                    .get_by_code(&store_ctx, &id_or_code)
                    .await?
                    .ok_or(CoreError::NotFound(format!(
                        "unable to find project with code: {}",
                        id_or_code
                    )))?
            }
        };

        Ok(project.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for ProjectService<D, C> {
    type CreateParams = ProjectCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.project.create;

    /// Creates a new Project, scoped to the provided workspace ID.
    async fn create(&self, ctx: &mut CoreCtx, params: ProjectCreateParams) -> CoreResult<Project> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        if let Some(code) = &params.code {
            if store.get_by_code(&store_ctx, code).await?.is_some() {
                return Err(CoreError::AlreadyExists(format!(
                    "Project code '{}' already exists in workspace {}",
                    code, params.workspace_id
                )));
            }
        }

        let n_project = params.into();

        let project_row = store.create(&store_ctx, n_project).await?;

        Ok(project_row.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for ProjectService<D, C> {
    type DescribeParams = ProjectDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.project.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: ProjectDescribeParams,
    ) -> CoreResult<Project> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let project = self.get_by_id_or_code(ctx, &params.id_or_code()?).await?;

        Ok(project)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for ProjectService<D, C> {
    type ListParams = ProjectListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.project.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: ProjectListParams,
    ) -> CoreResult<ListResponse<Project>> {
        let store = self.store();

        // validate params
        let list_options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
            .await?;

        // Combined query: tags (@> containment) + field filter
        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_with_tags_and_filter(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(list_options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let projects: Vec<Project> = data.into_iter().map(|el| el.into()).collect();

        Ok(ListResponse::new(projects, total, list_options))
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for ProjectService<D, C> {
    type UpdateParams = ProjectUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.project.update;

    async fn update(&self, ctx: &mut CoreCtx, params: ProjectUpdateParams) -> CoreResult<Project> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // check if project with code already exists
        if let Some(code) = &params.new_code {
            if let Ok(project) = self.get_by_id_or_code(ctx, &code).await {
                return Err(CoreError::AlreadyExists(format!(
                    "Project code '{}' already exists in workspace {}",
                    code,
                    ctx.scoped_ws_id()
                )));
            }
        }

        let project = self.get_by_id_or_code(ctx, &params.id_or_code()?).await?;
        let update_data: ProjectForUpdate = params.into();

        let project_row = store
            .update(&store_ctx, &project.id.into(), update_data)
            .await?;

        // TODO: invalidate auth cache
        // A project update may affect `auth_scope.project_id` cached in the
        // AuthCache entries of project-scoped memberships.
        Ok(project_row.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for ProjectService<D, C> {
    type DeleteParams = ProjectDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.project.delete;

    async fn delete(&self, ctx: &mut CoreCtx, params: ProjectDeleteParams) -> CoreResult<Project> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        let project = self.get_by_id_or_code(ctx, &params.id_or_code()?).await?;

        let deleted_row = store.delete(&store_ctx, &project.id.into()).await?;

        // TODO: invalidate auth cache
        // Deleting a project cascades to its memberships; their cached
        // AuthCache entries must be purged to avoid stale authorization.
        Ok(deleted_row.into())
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
                project::{ProjectConfig, ProjectMeta},
                workspace::WorkspaceRow,
            },
        },
    };
    use serial_test::serial;
    use uuid::Uuid;

    /// Builds a `ProjectRow` for the in-memory mock.
    fn project_row(id: Uuid, ws_id: Uuid) -> ProjectRow {
        ProjectRow {
            id: id.into(),
            workspace_id: ws_id,
            name: "test-project".to_string(),
            code: Some("proj-code".to_string()),
            description: None,
            owner: Uuid::nil().into(),
            config: ProjectConfig::default(),
            tags: vec![],
            meta: ProjectMeta::default(),
            audit: AuditFields::default(),
        }
    }

    /// Builds a `ProjectService` backed by an in-memory `MockDbx` + `MockChx`.
    fn mock_svc(dbx: MockDbx) -> Arc<ProjectService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.project.clone()
    }

    #[tokio::test]
    #[serial]
    async fn test_project_create() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // store.create
            .with_one::<ProjectRow>(project_row(project_id, ws_id));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["project:create"])?;

        let params = ProjectCreateParams {
            workspace_id: ws_id,
            name: "test-project".to_string(),
            code: None,
            description: None,
            owner: None,
            config: ProjectConfig::default(),
            tags: vec![],
            meta: ProjectMeta::default(),
        };

        let project = svc.create(&mut ctx, params).await?;

        assert_eq!(project.id, project_id);
        assert_eq!(project.workspace_id, ws_id);
        assert_eq!(project.name, "test-project");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_project_create_duplicate_code() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // get_by_code -> list -> existing project found
            .with_all::<ProjectRow>(vec![project_row(Uuid::new_v4(), ws_id)]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["project:create"])?;

        let params = ProjectCreateParams {
            workspace_id: ws_id,
            name: "dup".to_string(),
            code: Some("dup-code".to_string()),
            description: None,
            owner: None,
            config: ProjectConfig::default(),
            tags: vec![],
            meta: ProjectMeta::default(),
        };

        let res = svc.create(&mut ctx, params).await;

        assert!(
            matches!(res, Err(CoreError::AlreadyExists(_))),
            "duplicate project code must produce AlreadyExists"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_project_describe() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // store.get
            .with_optional::<ProjectRow>(Some(project_row(project_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["project:describe"])?;

        let project = svc
            .describe(
                &mut ctx,
                ProjectDescribeParams {
                    id: Some(project_id),
                    code: None,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(project.id, project_id);
        assert_eq!(project.workspace_id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_project_describe_missing_identifier() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["project:describe"])?;

        let res = svc
            .describe(
                &mut ctx,
                ProjectDescribeParams {
                    id: None,
                    code: None,
                    workspace_id: ws_id,
                },
            )
            .await;

        assert!(
            matches!(res, Err(CoreError::InvalidParams(_))),
            "describe without id or code must fail with InvalidParams"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_project_list() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            // .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
            //     id: ws_id.into(),
            //     ..Default::default()
            // }))
            // list_with_tags_and_filter
            .with_all::<ProjectRow>(vec![project_row(project_id, ws_id)])
            // count_with_tags_and_filter
            .with_one::<(i64,)>((1,))
            // hydrate_projects -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["project:list"])?;

        let res = svc
            .list(
                &mut ctx,
                ProjectListParams {
                    workspace_id: ws_id,
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id, project_id);
        assert_eq!(res.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_project_update() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            // .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
            //     id: ws_id.into(),
            //     ..Default::default()
            // }))
            // store.update
            .with_optional::<ProjectRow>(Some(project_row(project_id, ws_id)))
            .with_optional::<ProjectRow>(Some(project_row(project_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["project:update"])?;

        let params = ProjectUpdateParams {
            id: Some(project_id),
            code: None,
            workspace_id: ws_id,
            name: Some("renamed".to_string()),
            new_code: None,
            description: None,
            config: None,
            tags: None,
            meta: None,
        };

        let updated = svc.update(&mut ctx, params).await?;

        assert_eq!(updated.id, project_id);
        assert_eq!(updated.workspace_id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_project_delete() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let ws = WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        };

        let dbx = MockDbx::new()
            .with_optional::<ProjectRow>(Some(project_row(project_id, ws_id)))
            .with_optional::<ProjectRow>(Some(project_row(project_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["project:delete"])?;
        ctx.set_scoped_ws(ws.into());

        let deleted = svc
            .delete(
                &mut ctx,
                ProjectDeleteParams {
                    id: Some(project_id),
                    workspace_id: ws_id,
                    code: None,
                },
            )
            .await?;

        assert_eq!(deleted.id, project_id);
        assert_eq!(deleted.workspace_id, ws_id);

        Ok(())
    }
}

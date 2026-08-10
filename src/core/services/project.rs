use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

use crate::{
    cache::traits::CacheExecutor,
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
            auth::AuthValidator, permission::CANONICAL_PERMISSIONS, workspace::WorkspaceService,
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

    async fn hydrate_projects(
        &self,
        ctx: &mut CoreCtx,
        rows: Vec<ProjectRow>,
    ) -> CoreResult<Vec<Project>> {
        let mut workspaces: HashMap<Uuid, Workspace> = HashMap::new();

        let mut projects: Vec<Project> = Vec::with_capacity(rows.len());

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
            let project = Project::from_row_with_workspace(row, workspace.clone())?;
            projects.push(project);
        }

        Ok(projects)
    }

    /// Resolves a Project's DbId from either Uuid or code, enforcing workspace scoping.
    async fn get_project_id(
        &self,
        store_ctx: &StoreCtx,
        id: Option<Uuid>,
        code: Option<String>,
        workspace_id: Uuid,
    ) -> CoreResult<DbId> {
        let store = self.store();

        let id_db: DbId = match (id, code) {
            (Some(id), _) => id.into(), // If UUID provided, use it directly (store handles scope)
            (None, Some(code)) => {
                // If code provided, lookup by code and ensure it's in the correct workspace
                match store
                    .get_by_code(&store_ctx, &code, &workspace_id.into())
                    .await?
                {
                    Some(project_row) => project_row.id,
                    None => {
                        return Err(CoreError::StoreError(StoreError::EntityNotFound {
                            entity: "project".to_string(),
                            id: format!("code:'{}' in ws:'{}'", code, workspace_id),
                        }));
                    }
                }
            }
            (None, None) => {
                return Err(CoreError::InvalidParams(
                    "Project ID or code required for operation".to_string(),
                ));
            }
        };

        Ok(id_db)
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
            if store
                .get_by_code(&store_ctx, code, &params.workspace_id.into())
                .await?
                .is_some()
            {
                return Err(CoreError::AlreadyExists(format!(
                    "Project code '{}' already exists in workspace {}",
                    code, params.workspace_id
                )));
            }
        }

        let n_project = ProjectForCreate {
            workspace_id: params.workspace_id,
            name: params.name,
            code: params.code,
            description: params.description,
            config: params.config,
            tags: params.tags,
            meta: params.meta,
        };

        let project_row = store.create(&store_ctx, n_project).await?;

        Project::from_row_with_workspace(project_row, workspace)
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

        let id_db = self
            .get_project_id(&store_ctx, params.id, params.code, workspace.id)
            .await?;

        let project_row = store.get(&store_ctx, &id_db).await?;

        Project::from_row_with_workspace(project_row, workspace)
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

        let projects = self.hydrate_projects(ctx, data).await?;

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

        let id_db = self
            .get_project_id(&store_ctx, params.id, params.code.clone(), workspace.id)
            .await?;

        if let Some(new_code) = &params.new_code {
            if store
                .get_by_code(&store_ctx, new_code, &workspace.id.into())
                .await?
                .filter(|p| p.id != id_db) // Filter out the current project
                .is_some()
            {
                return Err(CoreError::AlreadyExists(format!(
                    "Project code '{}' already exists in workspace {}",
                    new_code, workspace.id
                )));
            }
        }

        let update_data = ProjectForUpdate {
            name: params.name,
            code: params.new_code, // Use the potentially changed code
            description: params.description,
            config: params.config,
            tags: params.tags,
            meta: params.meta,
        };

        let project_row = store.update(&store_ctx, &id_db, update_data).await?;

        Project::from_row_with_workspace(project_row, workspace)
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

        let id_db = self
            .get_project_id(&store_ctx, params.id, params.code, workspace.id)
            .await?;

        let deleted_row = store.delete(&store_ctx, &id_db).await?;

        Project::from_row_with_workspace(deleted_row, workspace)
    }
}

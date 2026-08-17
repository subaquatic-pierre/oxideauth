use std::sync::Arc;
use uuid::Uuid;

use crate::{
    cache::traits::CacheExecutor,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::{ListResponse, RequestFilterParams},
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
    validator: Arc<AuthValidator>,
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

    fn validator(&self) -> &AuthValidator {
        self.validator.as_ref()
    }

    fn is_scoped(&self) -> bool {
        true
    }
}

impl<D: DbExecutor, C: CacheExecutor> ProjectService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            ws_svc,
            validator,
        }
    }

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

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        if let Some(code) = &params.code {
            if store.get_by_code(&store_ctx, code).await?.is_some() {
                return Err(CoreError::AlreadyExists(format!(
                    "Project code '{}' already exists in workspace {}",
                    code,
                    ctx.scoped_ws_id()
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

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
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

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
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

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
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

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        let project = self.get_by_id_or_code(ctx, &params.id_or_code()?).await?;

        let deleted_row = store.delete(&store_ctx, &project.id.into()).await?;

        // TODO: invalidate auth cache
        // Deleting a project cascades to its memberships; their cached
        // AuthCache entries must be purged to avoid stale authorization.
        Ok(deleted_row.into())
    }
}


use axum::{
    extract::{rejection::JsonRejection, Extension, State},
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        error::CoreError,
        models::{
            list::{ListResponse, ListResponseMeta, RequestFilterParams, RequestListOptions},
            project::{
                Project, ProjectCreateParams, ProjectDeleteParams, ProjectDescribeParams,
                ProjectListParams, ProjectUpdateParams,
            },
        },
        traits::service::{
            CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
            CoreModelListService, CoreModelUpdateService,
        },
    },
    store::entities::project::ProjectFilter,
    web::{
        dtos::project::{
            ProjectCreateReq, ProjectDeleteReq, ProjectDeleteRes, ProjectDescribeReq,
            ProjectDescribeRes, ProjectListReq, ProjectListRes, ProjectUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        middlewares::cors::build_cors,
        response::WebResponse,
    },
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Describe Project ---
#[axum::debug_handler]
pub async fn describe_project(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProjectDescribeReq>,
) -> JsonResResult<WebResponse<ProjectDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.project();

    let params: ProjectDescribeParams = body.into();

    let project = svc.describe(&mut ctx, params).await?;

    let project_res: ProjectDescribeRes = project.into();

    info!("describe_workspace - CTX: {ctx:#?}");
    WebResponse::json(project_res)
}

// --- List Projects ---
#[axum::debug_handler]
pub async fn list_workspaces(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProjectListReq>,
) -> JsonResResult<WebResponse<ProjectListRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.project();

    let params: ProjectListParams = body.into();
    let res = svc.list(&mut ctx, params).await?;

    // Map the vector of Project entities to the vector of DTOs
    let projects: Vec<ProjectDescribeRes> =
        res.data.into_iter().map(ProjectDescribeRes::from).collect();

    let res = ProjectListRes {
        projects,
        metadata: res.metadata,
    };

    WebResponse::json(res)
}

// --- Create Project ---
#[axum::debug_handler]
pub async fn create_workspace(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProjectCreateReq>,
) -> JsonResResult<WebResponse<ProjectDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.project();

    let params: ProjectCreateParams = body.into();

    let project = svc.create(&mut ctx, params).await?;

    let project_res: ProjectDescribeRes = project.into();

    info!("create_workspace - CTX: {ctx:#?}");
    WebResponse::json(project_res)
}

// --- Update Project ---
#[axum::debug_handler]
pub async fn update_workspace(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProjectUpdateReq>,
) -> JsonResResult<WebResponse<ProjectDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.project();

    let params: ProjectUpdateParams = body.into();

    let project = svc.update(&mut ctx, params).await?;

    let project_res: ProjectDescribeRes = project.into();

    info!("update_workspace - CTX: {ctx:#?}");
    WebResponse::json(project_res)
}

// --- Delete Project ---
#[axum::debug_handler]
pub async fn delete_workspace(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProjectDeleteReq>,
) -> JsonResResult<WebResponse<ProjectDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.project();

    let params: ProjectDeleteParams = body.into();

    // The service returns the deleted Project entity
    let project = svc.delete(&mut ctx, params).await?;

    // Convert the deleted Project entity into the response DTO
    let res: ProjectDeleteRes = project.into();

    info!("delete_workspace - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Project Router ---
pub struct ProjectRouter;

impl ProjectRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_project))
            .route("/create", post(create_workspace))
            .route("/list", post(list_workspaces))
            .route("/update", post(update_workspace))
            .route("/delete", post(delete_workspace))
    }
}

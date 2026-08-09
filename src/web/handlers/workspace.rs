use axum::{
    Json, Router,
    extract::{Extension, State, rejection::JsonRejection},
    response::IntoResponse,
    routing::{delete, get, patch, post},
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
            workspace::{
                Workspace, WorkspaceCreateParams, WorkspaceDeleteParams, WorkspaceDescribeParams,
                WorkspaceListParams, WorkspaceUpdateParams,
            },
        },
        traits::service::*,
    },
    store::entities::workspace::WorkspaceFilter,
    web::{
        dtos::workspace::{
            WorkspaceCreateReq, WorkspaceDeleteReq, WorkspaceDeleteRes, WorkspaceDescribeReq,
            WorkspaceDescribeRes, WorkspaceListReq, WorkspaceListRes, WorkspaceUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        middlewares::cors::build_cors,
        response::WebResponse,
    },
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Describe Workspace ---
#[axum::debug_handler]
pub async fn describe_workspace(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<WorkspaceDescribeReq>,
) -> JsonResResult<WebResponse<WorkspaceDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.workspace();

    let params: WorkspaceDescribeParams = body.into();

    let ws = svc.describe(&mut ctx, params).await?;

    let ws_res: WorkspaceDescribeRes = ws.into();

    // info!("describe_workspace - CTX: {ctx:#?}");
    WebResponse::json(ws_res)
}

// --- List Workspaces ---
#[axum::debug_handler]
pub async fn list_workspaces(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<WorkspaceListReq>,
) -> JsonResResult<WebResponse<WorkspaceListRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.workspace();

    let params: WorkspaceListParams = body.into();
    let res = svc.list(&mut ctx, params).await?;

    // Map the vector of Workspace entities to the vector of DTOs
    let workspaces: Vec<WorkspaceDescribeRes> = res
        .data
        .into_iter()
        .map(WorkspaceDescribeRes::from)
        .collect();

    let res = WorkspaceListRes {
        workspaces,
        metadata: res.metadata,
    };

    WebResponse::json(res)
}

// --- Create Workspace ---
#[axum::debug_handler]
pub async fn create_workspace(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<WorkspaceCreateReq>,
) -> JsonResResult<WebResponse<WorkspaceDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.workspace();

    let params: WorkspaceCreateParams = body.into();

    let ws = svc.create(&mut ctx, params).await?;

    let ws_res: WorkspaceDescribeRes = ws.into();

    // info!("create_workspace - CTX: {ctx:#?}");
    WebResponse::json(ws_res)
}

// --- Update Workspace ---
#[axum::debug_handler]
pub async fn update_workspace(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<WorkspaceUpdateReq>,
) -> JsonResResult<WebResponse<WorkspaceDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.workspace();

    let params: WorkspaceUpdateParams = body.into();

    let ws = svc.update(&mut ctx, params).await?;

    let ws_res: WorkspaceDescribeRes = ws.into();

    // info!("update_workspace - CTX: {ctx:#?}");
    WebResponse::json(ws_res)
}

// --- Delete Workspace ---
#[axum::debug_handler]
pub async fn delete_workspace(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<WorkspaceDeleteReq>,
) -> JsonResResult<WebResponse<WorkspaceDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.workspace();

    let params: WorkspaceDeleteParams = body.into();

    // The service returns the deleted Workspace entity
    let ws = svc.delete(&mut ctx, params).await?;

    // Convert the deleted Workspace entity into the response DTO
    let res: WorkspaceDeleteRes = ws.into();

    // info!("delete_workspace - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Workspace Router ---
pub struct WorkspaceRouter;

impl WorkspaceRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_workspace))
            .route("/create", post(create_workspace))
            .route("/list", post(list_workspaces))
            .route("/update", post(update_workspace))
            .route("/delete", post(delete_workspace))
    }
}

use axum::{Json, Router, extract::Extension, routing::post};
use tracing::info;

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        models::permission::{
            PermissionCreateParams, PermissionDeleteParams, PermissionDescribeParams,
            PermissionListParams, PermissionUpdateParams,
        },
        traits::{
            params::IntoParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelUpdateService,
            },
        },
    },
    web::{
        dtos::permission::{
            PermissionCreateReq, PermissionDeleteReq, PermissionDeleteRes, PermissionDescribeReq,
            PermissionDescribeRes, PermissionListReq, PermissionListRes, PermissionUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        response::WebResponse,
    },
};

// --- Describe Permission ---
#[axum::debug_handler]
pub async fn describe_permission(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PermissionDescribeReq>,
) -> JsonResResult<WebResponse<PermissionDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.permission.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PermissionDescribeParams = body.into_params(ws_id)?;
    let perm = svc.describe(&mut ctx, params).await?;
    let res: PermissionDescribeRes = perm.into();

    // info!("describe_permission - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- List Permissions ---
#[axum::debug_handler]
pub async fn list_permissions(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PermissionListReq>,
) -> JsonResResult<WebResponse<PermissionListRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.permission.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PermissionListParams = body.into_params(ws_id)?;
    let list_res = svc.list(&mut ctx, params).await?;

    let permissions: Vec<PermissionDescribeRes> = list_res
        .data
        .into_iter()
        .map(PermissionDescribeRes::from)
        .collect();

    let res = PermissionListRes {
        permissions,
        metadata: list_res.metadata,
    };

    WebResponse::json(res)
}

// --- Create Permission ---
#[axum::debug_handler]
pub async fn create_permission(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PermissionCreateReq>,
) -> JsonResResult<WebResponse<PermissionDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.permission.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PermissionCreateParams = body.into_params(ws_id)?;
    let perm = svc.create(&mut ctx, params).await?;
    let res: PermissionDescribeRes = perm.into();

    // info!("create_permission - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Update Permission ---
#[axum::debug_handler]
pub async fn update_permission(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PermissionUpdateReq>,
) -> JsonResResult<WebResponse<PermissionDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.permission.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PermissionUpdateParams = body.into_params(ws_id)?;
    let perm = svc.update(&mut ctx, params).await?;
    let res: PermissionDescribeRes = perm.into();

    // info!("update_permission - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Delete Permission ---
#[axum::debug_handler]
pub async fn delete_permission(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PermissionDeleteReq>,
) -> JsonResResult<WebResponse<PermissionDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.permission.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PermissionDeleteParams = body.into_params(ws_id)?;
    let perm = svc.delete(&mut ctx, params).await?;

    let res = PermissionDeleteRes { id: perm.id };

    // info!("delete_permission - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Permission Router ---
pub struct PermissionRouter;

impl PermissionRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_permission))
            .route("/create", post(create_permission))
            .route("/list", post(list_permissions))
            .route("/update", post(update_permission))
            .route("/delete", post(delete_permission))
    }
}

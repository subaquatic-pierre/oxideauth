use axum::{
    extract::Extension,
    routing::post,
    Json, Router,
};
use tracing::info;

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        models::{
            role::{
                RoleCreateParams, RoleDeleteParams, RoleDescribeParams, RoleListParams,
                RoleUpdateParams,
            },
        },
        traits::service::{
            CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
            CoreModelListService, CoreModelUpdateService,
        },
    },
    web::{
        dtos::role::{
            RoleCreateReq, RoleDeleteReq, RoleDeleteRes, RoleDescribeReq, RoleDescribeRes,
            RoleListReq, RoleListRes, RoleUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        response::WebResponse,
    },
};

// --- Describe Role ---
#[axum::debug_handler]
pub async fn describe_role(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<RoleDescribeReq>,
) -> JsonResResult<WebResponse<RoleDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.role();

    let params: RoleDescribeParams = body.into();
    let role = svc.describe(&mut ctx, params).await?;
    let res: RoleDescribeRes = role.into();

    info!("describe_role - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- List Roles ---
#[axum::debug_handler]
pub async fn list_roles(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<RoleListReq>,
) -> JsonResResult<WebResponse<RoleListRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.role();

    let params: RoleListParams = body.into();
    let list_res = svc.list(&mut ctx, params).await?;

    let roles: Vec<RoleDescribeRes> = list_res
        .data
        .into_iter()
        .map(RoleDescribeRes::from)
        .collect();

    let res = RoleListRes {
        roles,
        metadata: list_res.metadata,
    };

    WebResponse::json(res)
}

// --- Create Role ---
#[axum::debug_handler]
pub async fn create_role(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<RoleCreateReq>,
) -> JsonResResult<WebResponse<RoleDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.role();

    let params: RoleCreateParams = body.into();
    let role = svc.create(&mut ctx, params).await?;
    let res: RoleDescribeRes = role.into();

    info!("create_role - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Update Role ---
#[axum::debug_handler]
pub async fn update_role(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<RoleUpdateReq>,
) -> JsonResResult<WebResponse<RoleDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.role();

    let params: RoleUpdateParams = body.into();
    let role = svc.update(&mut ctx, params).await?;
    let res: RoleDescribeRes = role.into();

    info!("update_role - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Delete Role ---
#[axum::debug_handler]
pub async fn delete_role(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<RoleDeleteReq>,
) -> JsonResResult<WebResponse<RoleDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.role();

    let params: RoleDeleteParams = body.into();
    let role = svc.delete(&mut ctx, params).await?;

    let res = RoleDeleteRes { id: role.id };

    info!("delete_role - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Role Router ---
pub struct RoleRouter;

impl RoleRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_role))
            .route("/create", post(create_role))
            .route("/list", post(list_roles))
            .route("/update", post(update_role))
            .route("/delete", post(delete_role))
    }
}

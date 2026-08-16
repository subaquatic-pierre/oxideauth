use axum::{Json, Router, extract::Extension, routing::post};
use tracing::info;

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        models::policy::{
            PolicyCreateParams, PolicyDeleteParams, PolicyDescribeParams, PolicyListParams,
            PolicyUpdateParams,
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
        dtos::policy::{
            PolicyCreateReq, PolicyDeleteReq, PolicyDeleteRes, PolicyDescribeReq,
            PolicyDescribeRes, PolicyListReq, PolicyListRes, PolicyUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        response::WebResponse,
    },
};

// --- Describe Policy ---
#[axum::debug_handler]
pub async fn describe_policy(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PolicyDescribeReq>,
) -> JsonResResult<WebResponse<PolicyDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.policy.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PolicyDescribeParams = body.into_params(ws_id)?;
    let policy = svc.describe(&mut ctx, params).await?;
    let res: PolicyDescribeRes = policy.into();

    // info!("describe_policy - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- List Policies ---
#[axum::debug_handler]
pub async fn list_policies(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PolicyListReq>,
) -> JsonResResult<WebResponse<PolicyListRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.policy.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PolicyListParams = body.into_params(ws_id)?;
    let list_res = svc.list(&mut ctx, params).await?;

    let policies: Vec<PolicyDescribeRes> = list_res
        .data
        .into_iter()
        .map(PolicyDescribeRes::from)
        .collect();

    let res = PolicyListRes {
        policies,
        metadata: list_res.metadata,
    };

    WebResponse::json(res)
}

// --- Create Policy ---
#[axum::debug_handler]
pub async fn create_policy(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PolicyCreateReq>,
) -> JsonResResult<WebResponse<PolicyDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.policy.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PolicyCreateParams = body.into_params(ws_id)?;
    let policy = svc.create(&mut ctx, params).await?;
    let res: PolicyDescribeRes = policy.into();

    // info!("create_policy - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Update Policy ---
#[axum::debug_handler]
pub async fn update_policy(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PolicyUpdateReq>,
) -> JsonResResult<WebResponse<PolicyDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.policy.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PolicyUpdateParams = body.into_params(ws_id)?;
    let policy = svc.update(&mut ctx, params).await?;
    let res: PolicyDescribeRes = policy.into();

    // info!("update_policy - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Delete Policy ---
#[axum::debug_handler]
pub async fn delete_policy(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<PolicyDeleteReq>,
) -> JsonResResult<WebResponse<PolicyDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.policy.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: PolicyDeleteParams = body.into_params(ws_id)?;
    let policy = svc.delete(&mut ctx, params).await?;

    let res = PolicyDeleteRes { id: policy.id };

    // info!("delete_policy - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Policy Router ---
pub struct PolicyRouter;

impl PolicyRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_policy))
            .route("/create", post(create_policy))
            .route("/list", post(list_policies))
            .route("/update", post(update_policy))
            .route("/delete", post(delete_policy))
    }
}

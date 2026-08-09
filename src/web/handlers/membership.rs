use axum::{Json, Router, extract::Extension, routing::post};
use tracing::info;

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        models::membership::{
            MembershipCreateParams, MembershipDeleteParams, MembershipDescribeParams,
            MembershipListParams, MembershipUpdateParams,
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
        dtos::membership::{
            MembershipCreateReq, MembershipDeleteReq, MembershipDeleteRes, MembershipDescribeReq,
            MembershipDescribeRes, MembershipListReq, MembershipListRes, MembershipUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        response::WebResponse,
    },
};

// --- Describe Membership ---
#[axum::debug_handler]
pub async fn describe_membership(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<MembershipDescribeReq>,
) -> JsonResResult<WebResponse<MembershipDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.membership();

    let ws_id = ctx.scoped_ws_id();
    let params: MembershipDescribeParams = body.into_params(ws_id)?;
    let m = svc.describe(&mut ctx, params).await?;
    let res: MembershipDescribeRes = m.into();

    // info!("describe_membership - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- List Memberships ---
#[axum::debug_handler]
pub async fn list_memberships(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<MembershipListReq>,
) -> JsonResResult<WebResponse<MembershipListRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.membership();

    let ws_id = ctx.scoped_ws_id();
    let params: MembershipListParams = body.into_params(ws_id)?;
    let list_res = svc.list(&mut ctx, params).await?;

    let memberships: Vec<MembershipDescribeRes> = list_res
        .data
        .into_iter()
        .map(MembershipDescribeRes::from)
        .collect();

    let res = MembershipListRes {
        memberships,
        metadata: list_res.metadata,
    };

    WebResponse::json(res)
}

// --- Create Membership ---
#[axum::debug_handler]
pub async fn create_membership(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<MembershipCreateReq>,
) -> JsonResResult<WebResponse<MembershipDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.membership();

    let ws_id = ctx.scoped_ws_id();
    let params: MembershipCreateParams = body.into_params(ws_id)?;
    let m = svc.create(&mut ctx, params).await?;
    let res: MembershipDescribeRes = m.into();

    // info!("create_membership - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Update Membership ---
#[axum::debug_handler]
pub async fn update_membership(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<MembershipUpdateReq>,
) -> JsonResResult<WebResponse<MembershipDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.membership();

    let ws_id = ctx.scoped_ws_id();
    let params: MembershipUpdateParams = body.into_params(ws_id)?;
    let m = svc.update(&mut ctx, params).await?;
    let res: MembershipDescribeRes = m.into();

    // info!("update_membership - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Delete Membership ---
#[axum::debug_handler]
pub async fn delete_membership(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<MembershipDeleteReq>,
) -> JsonResResult<WebResponse<MembershipDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.membership();

    let ws_id = ctx.scoped_ws_id();
    let params: MembershipDeleteParams = body.into_params(ws_id)?;
    let m = svc.delete(&mut ctx, params).await?;

    let res = MembershipDeleteRes { id: m.id };

    // info!("delete_membership - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Membership Router ---
pub struct MembershipRouter;

impl MembershipRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_membership))
            .route("/create", post(create_membership))
            .route("/list", post(list_memberships))
            .route("/update", post(update_membership))
            .route("/delete", post(delete_membership))
    }
}

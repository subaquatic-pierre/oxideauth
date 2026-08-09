use axum::{Json, Router, extract::Extension, routing::post};
use tracing::info;

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        models::credential::{
            CredentialDeleteParams, CredentialDescribeParams, CredentialListParams,
            CredentialUpdateParams,
        },
        traits::service::{
            CoreModelDeleteService, CoreModelDescribeService, CoreModelListService,
            CoreModelUpdateService,
        },
    },
    web::{
        dtos::credential::{
            CredentialDeleteReq, CredentialDeleteRes, CredentialDescribeReq, CredentialDescribeRes,
            CredentialListReq, CredentialListRes, CredentialUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        response::WebResponse,
    },
};

// --- Describe Credential ---
#[axum::debug_handler]
pub async fn describe_credential(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<CredentialDescribeReq>,
) -> JsonResResult<WebResponse<CredentialDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.credential();

    let params: CredentialDescribeParams = body.into();
    let c = svc.describe(&mut ctx, params).await?;
    let res: CredentialDescribeRes = c.into();

    info!("describe_credential - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- List Credentials ---
#[axum::debug_handler]
pub async fn list_credentials(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<CredentialListReq>,
) -> JsonResResult<WebResponse<CredentialListRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.credential();

    let params: CredentialListParams = body.into();
    let list_res = svc.list(&mut ctx, params).await?;

    let credentials: Vec<CredentialDescribeRes> = list_res
        .data
        .into_iter()
        .map(CredentialDescribeRes::from)
        .collect();

    let res = CredentialListRes {
        credentials,
        metadata: list_res.metadata,
    };

    WebResponse::json(res)
}

// --- Update Credential ---
#[axum::debug_handler]
pub async fn update_credential(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<CredentialUpdateReq>,
) -> JsonResResult<WebResponse<CredentialDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.credential();

    let params: CredentialUpdateParams = body.into();
    let c = svc.update(&mut ctx, params).await?;
    let res: CredentialDescribeRes = c.into();

    // info!("update_credential - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Delete Credential ---
#[axum::debug_handler]
pub async fn delete_credential(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<CredentialDeleteReq>,
) -> JsonResResult<WebResponse<CredentialDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.credential();

    let params: CredentialDeleteParams = body.into();
    let c = svc.delete(&mut ctx, params).await?;

    let res = CredentialDeleteRes { id: c.id };

    // info!("delete_credential - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// NOTE: No create_credential handler — create is excluded per spec

// --- Credential Router ---
pub struct CredentialRouter;

impl CredentialRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_credential))
            .route("/list", post(list_credentials))
            .route("/update", post(update_credential))
            .route("/delete", post(delete_credential))
    }
}

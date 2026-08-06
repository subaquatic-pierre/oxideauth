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
            token::{
                TokenDeleteParams, TokenDescribeParams, TokenListParams,
            },
        },
        traits::service::{
            CoreModelDeleteService, CoreModelDescribeService, CoreModelListService,
        },
    },
    web::{
        dtos::token::{
            TokenDeleteReq, TokenDeleteRes, TokenDescribeReq, TokenDescribeRes, TokenListReq,
            TokenListRes,
        },
        error::{JsonReqResult, JsonResResult},
        response::WebResponse,
    },
};

// --- Describe Token ---
#[axum::debug_handler]
pub async fn describe_token(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<TokenDescribeReq>,
) -> JsonResResult<WebResponse<TokenDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.token();

    let params: TokenDescribeParams = body.into();
    let t = svc.describe(&mut ctx, params).await?;
    let res: TokenDescribeRes = t.into();

    info!("describe_token - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- List Tokens ---
#[axum::debug_handler]
pub async fn list_tokens(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<TokenListReq>,
) -> JsonResResult<WebResponse<TokenListRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.token();

    let params: TokenListParams = body.into();
    let list_res = svc.list(&mut ctx, params).await?;

    let tokens: Vec<TokenDescribeRes> = list_res
        .data
        .into_iter()
        .map(TokenDescribeRes::from)
        .collect();

    let res = TokenListRes {
        tokens,
        metadata: list_res.metadata,
    };

    WebResponse::json(res)
}

// --- Delete Token ---
#[axum::debug_handler]
pub async fn delete_token(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<TokenDeleteReq>,
) -> JsonResResult<WebResponse<TokenDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.token();

    let params: TokenDeleteParams = body.into();
    let t = svc.delete(&mut ctx, params).await?;

    let res = TokenDeleteRes { id: t.id };

    info!("delete_token - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// NOTE: No create_token handler — create is excluded per spec
// NOTE: No update_token handler — update is excluded per spec

// --- Token Router ---
pub struct TokenRouter;

impl TokenRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_token))
            .route("/list", post(list_tokens))
            .route("/delete", post(delete_token))
    }
}

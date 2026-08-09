use axum::{
    Json, Router,
    extract::{Extension, State, rejection::JsonRejection},
    response::IntoResponse,
    routing::{get, post},
};
use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        error::CoreError,
        models::{
            account::{
                Account, AccountCreateParams, AccountDeleteParams, AccountDescribeParams,
                AccountListParams, AccountUpdateParams,
            },
            list::{ListResponse, ListResponseMeta, RequestFilterParams, RequestListOptions},
        },
        traits::{
            params::IntoParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelUpdateService,
            },
        },
    },
    store::entities::account::AccountFilter,
    web::{
        dtos::account::{
            AccountCreateReq, AccountDeleteReq, AccountDeleteRes, AccountDescribeReq,
            AccountDescribeRes, AccountListReq, AccountListRes, AccountUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        middlewares::cors::build_cors,
        response::WebResponse,
    },
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[axum::debug_handler]
pub async fn describe_account(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<AccountDescribeReq>,
) -> JsonResResult<WebResponse<AccountDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.account();

    let ws_id = ctx.scoped_ws_id();
    let params: AccountDescribeParams = body.into_params(ws_id)?;

    let acc = svc.describe(&mut ctx, params).await?;

    let acc_res = acc.into();

    WebResponse::json(acc_res)
}

#[axum::debug_handler]
pub async fn list_accounts(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<AccountListReq>,
) -> JsonResResult<WebResponse<AccountListRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.account();

    let ws_id = ctx.scoped_ws_id();
    let params: AccountListParams = body.into_params(ws_id)?;
    let res = svc.list(&mut ctx, params).await?;

    let res = AccountListRes {
        accounts: res.data,
        metadata: res.metadata,
    };

    WebResponse::json(res)
}

#[axum::debug_handler]
pub async fn create_account(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<AccountCreateReq>,
) -> JsonResResult<WebResponse<AccountDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.account();

    let ws_id = ctx.scoped_ws_id();
    let params: AccountCreateParams = body.into_params(ws_id)?;

    let acc = svc.create(&mut ctx, params).await?;

    let acc_res = acc.into();

    // info!("create_account - CTX: {ctx:#?}");

    WebResponse::json(acc_res)
}

#[axum::debug_handler]
pub async fn delete_account(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<AccountDeleteReq>,
) -> JsonResResult<WebResponse<AccountDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.account();

    let ws_id = ctx.scoped_ws_id();
    let params: AccountDeleteParams = body.into_params(ws_id)?;

    let acc = svc.delete(&mut ctx, params).await?;

    let res = AccountDeleteRes { id: acc.id };

    // info!("delete_account - CTX: {ctx:#?}");
    WebResponse::json(res)
}

#[axum::debug_handler]
pub async fn update_account(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<AccountUpdateReq>,
) -> JsonResResult<WebResponse<AccountDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.account();

    let ws_id = ctx.scoped_ws_id();
    let params: AccountUpdateParams = body.into_params(ws_id)?;

    let acc = svc.update(&mut ctx, params).await?;

    let acc_res: AccountDescribeRes = acc.into();

    // info!("update_account - CTX: {ctx:#?}");
    WebResponse::json(acc_res)
}

pub struct AccountRouter;

impl AccountRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_account))
            .route("/create", post(create_account))
            .route("/list", post(list_accounts))
            .route("/update", post(update_account))
            .route("/delete", post(delete_account))
    }
}

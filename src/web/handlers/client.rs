use axum::{Json, Router, extract::Extension, routing::post};
use tracing::info;

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        models::client::{
            ClientCreateParams, ClientDeleteParams, ClientDescribeParams, ClientListParams,
            ClientRegenerateSecretParams, ClientUpdateParams, ClientValidateParams,
        },
        services::client::ClientSecret,
        traits::{
            params::IntoParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelUpdateService,
            },
        },
    },
    web::{
        dtos::client::{
            ClientCreateReq, ClientCreateRes, ClientDeleteReq, ClientDeleteRes, ClientDescribeReq,
            ClientDescribeRes, ClientListReq, ClientListRes, ClientRegenerateSecretReq,
            ClientRegenerateSecretRes, ClientUpdateReq, ClientValidateReq, ClientValidateRes,
        },
        error::{JsonReqResult, JsonResResult},
        response::WebResponse,
    },
};

// --- Create Client ---
#[axum::debug_handler]
pub async fn create_client(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ClientCreateReq>,
) -> JsonResResult<WebResponse<ClientCreateRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.client.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ClientCreateParams = body.into_params(ws_id)?;
    let client = svc.create(&mut ctx, params).await?;

    // TODO: need to return secret somehow - for now, model doesn't expose it
    let res = ClientCreateRes::from(client);

    // info!("create_client - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- List Clients ---
#[axum::debug_handler]
pub async fn list_clients(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ClientListReq>,
) -> JsonResResult<WebResponse<ClientListRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.client.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ClientListParams = body.into_params(ws_id)?;
    let list_res = svc.list(&mut ctx, params).await?;

    let clients: Vec<ClientDescribeRes> = list_res
        .data
        .into_iter()
        .map(ClientDescribeRes::from)
        .collect();

    let res = ClientListRes {
        clients,
        metadata: list_res.metadata,
    };

    // info!("list_clients - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Validate Client ---
#[axum::debug_handler]
pub async fn validate_client(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ClientValidateReq>,
) -> JsonResResult<WebResponse<ClientValidateRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.client.clone();
    let ws_id = ctx.scoped_ws_id();
    let params: ClientValidateParams = body.into_params(ws_id)?;
    let authorized = svc.validate(&mut ctx, params).await.unwrap_or(false); // Any error → not authorized
    let res = ClientValidateRes { authorized };
    WebResponse::json(res)
}

// --- Describe Client ---
#[axum::debug_handler]
pub async fn describe_client(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ClientDescribeReq>,
) -> JsonResResult<WebResponse<ClientDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.client.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ClientDescribeParams = body.into_params(ws_id)?;
    let client = svc.describe(&mut ctx, params).await?;
    let res: ClientDescribeRes = client.into();

    // info!("describe_client - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Update Client ---
#[axum::debug_handler]
pub async fn update_client(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ClientUpdateReq>,
) -> JsonResResult<WebResponse<ClientDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.client.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ClientUpdateParams = body.into_params(ws_id)?;
    let client = svc.update(&mut ctx, params).await?;
    let res: ClientDescribeRes = client.into();

    // info!("update_client - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Delete Client ---
#[axum::debug_handler]
pub async fn delete_client(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ClientDeleteReq>,
) -> JsonResResult<WebResponse<ClientDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.client.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ClientDeleteParams = body.into_params(ws_id)?;
    let client = svc.delete(&mut ctx, params).await?;

    let res = ClientDeleteRes { id: client.id };

    // info!("delete_client - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Regenerate Client Secret ---
#[axum::debug_handler]
pub async fn regenerate_secret_client(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ClientRegenerateSecretReq>,
) -> JsonResResult<WebResponse<ClientRegenerateSecretRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.client.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ClientRegenerateSecretParams = body.into_params(ws_id)?;
    let client_secret = svc.regenerate_secret(&mut ctx, params).await?;

    let res = ClientRegenerateSecretRes {
        id: client_secret.client.id,
        secret: client_secret.plaintext_secret,
    };

    // info!("regenerate_secret_client - CTX: {ctx:#?}");
    WebResponse::json(res)
}

// --- Client Router ---
pub struct ClientRouter;

impl ClientRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/create", post(create_client))
            .route("/list", post(list_clients))
            .route("/validate", post(validate_client))
            .route("/describe", post(describe_client))
            .route("/update", post(update_client))
            .route("/delete", post(delete_client))
            .route("/regenerate-secret", post(regenerate_secret_client))
    }
}

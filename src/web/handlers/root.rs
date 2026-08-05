use axum::{
    extract::{Extension, State},
    routing::get,
    Router,
};
use std::sync::Arc;
use tracing::info;

use crate::{
    app::{App, AppState},
    core::{ctx::CoreCtx, error::CoreError},
    web::{error::JsonResResult, middlewares::cors::build_cors, response::WebResponse},
};

pub async fn index_handler(
    ctx: Extension<CoreCtx>,
    app: Extension<App>,
) -> JsonResResult<WebResponse<String>> {
    info!("root_handler - CTX: {ctx:#?}");
    WebResponse::json("Hello, World".to_string())
}

pub async fn health_check_handler(
    ctx: Extension<CoreCtx>,
    app: Extension<App>,
) -> JsonResResult<WebResponse<String>> {
    info!("health_check_handler - CTX: {ctx:#?}");
    WebResponse::json("Healthy".to_string())
}

pub struct RootRouter;

impl RootRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/", get(index_handler))
            .route("/health-check", get(health_check_handler))
    }
}

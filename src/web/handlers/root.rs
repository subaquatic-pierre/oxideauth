use axum::{
    extract::Extension,
    routing::get,
    Router,
};
use tracing::info;

use crate::{
    app::App,
    web::{error::JsonResResult, response::WebResponse},
};

pub async fn index_handler() -> JsonResResult<WebResponse<String>> {
    info!("index_handler");
    WebResponse::json("Hello, World".to_string())
}

pub async fn health_check_handler() -> JsonResResult<WebResponse<String>> {
    info!("health_check_handler");
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

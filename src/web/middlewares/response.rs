use axum::http::{Method, Uri};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde_json::{json, to_value};
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

use crate::core::ctx::CoreCtx;
use crate::web::middlewares::request::ReqStamp;

pub struct ResponseMw;

impl ResponseMw {
    pub async fn response_map_handler(
        stamp: Extension<ReqStamp>,
        // ctx: Extension<CoreCtx>,
        uri: Uri,
        method: Method,
        res: Response,
    ) -> Response {
        // debug!(
        //     "{:<12} - mw_reponse_map - {ctx:#?}, REQ STAMP {stamp:#?}",
        //     "RES_MAPPER"
        // );

        // add logging if needed

        res
    }
}

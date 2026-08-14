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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::time::now_utc;
    use crate::web::middlewares::request::ReqStamp;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, StatusCode, Uri};
    use axum::Extension;

    #[tokio::test]
    async fn test_response_map_handler_passes_response_through() {
        let stamp = ReqStamp {
            id: Uuid::new_v4(),
            ts: now_utc(),
        };
        let uri: Uri = "/resource".parse().unwrap();
        let res = Response::new(Body::from("hello body"));

        let out =
            ResponseMw::response_map_handler(Extension(stamp), uri, Method::GET, res).await;
        assert_eq!(out.status(), StatusCode::OK);

        let bytes = to_bytes(out.into_body(), 4096).await.unwrap();
        assert_eq!(&bytes[..], b"hello body");
    }

    #[tokio::test]
    async fn test_response_map_handler_preserves_post_response() {
        let stamp = ReqStamp {
            id: Uuid::new_v4(),
            ts: now_utc(),
        };
        let uri: Uri = "/thing".parse().unwrap();
        let res = Response::new(Body::from(r#"{"ok":true}"#));

        let out =
            ResponseMw::response_map_handler(Extension(stamp), uri, Method::POST, res).await;
        assert_eq!(out.status(), StatusCode::OK);

        let bytes = to_bytes(out.into_body(), 4096).await.unwrap();
        assert_eq!(&bytes[..], br#"{"ok":true}"#);
    }
}

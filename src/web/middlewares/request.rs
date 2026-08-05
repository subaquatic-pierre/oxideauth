use std::error::Error;

use axum::async_trait;
use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use time::OffsetDateTime;
use tracing::debug;
use uuid::Uuid;

use crate::utils::time::now_utc;
use crate::web::error::WebError;

#[derive(Debug, Clone)]
pub struct ReqStamp {
    pub id: Uuid,
    pub ts: OffsetDateTime,
}

pub struct RequestMw;

impl RequestMw {
    pub async fn request_map_handler(mut req: Request<Body>, next: Next) -> Response {
        let time_in = now_utc();
        let uuid = Uuid::new_v4();

        req.extensions_mut().insert(ReqStamp {
            id: uuid,
            ts: time_in,
        });

        next.run(req).await
    }
}

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for ReqStamp {
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, WebError> {
        debug!("{:<12} - ReqStamp", "EXTRACTOR");

        parts
            .extensions
            .get::<ReqStamp>()
            .cloned()
            .ok_or(WebError::ReqStampNotInReqExt)
    }
}

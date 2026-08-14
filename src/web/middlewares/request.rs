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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::time::now_utc;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::http::request::Parts;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_reqstamp_extractor_missing_extension() {
        let (parts, _) = Request::new(Body::empty()).into_parts();
        let mut parts: Parts = parts;
        let res = ReqStamp::from_request_parts(&mut parts, &()).await;
        assert!(matches!(res, Err(WebError::ReqStampNotInReqExt)));
    }

    #[tokio::test]
    async fn test_reqstamp_extractor_present() {
        let stamp = ReqStamp {
            id: Uuid::new_v4(),
            ts: now_utc(),
        };
        let (mut parts, _) = Request::new(Body::empty()).into_parts();
        parts.extensions.insert(stamp.clone());

        let res = ReqStamp::from_request_parts(&mut parts, &()).await;
        let got = res.unwrap();
        assert_eq!(got.id, stamp.id);
    }

    #[tokio::test]
    async fn test_request_map_handler_inserts_reqstamp() {
        async fn read_stamp(stamp: ReqStamp) -> String {
            stamp.id.to_string()
        }

        let app = Router::new()
            .route("/", get(read_stamp))
            .layer(middleware::from_fn(RequestMw::request_map_handler));

        let res = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
        let id_str = std::str::from_utf8(&bytes).unwrap();
        assert!(Uuid::parse_str(id_str).is_ok(), "expected a uuid, got: {id_str}");
    }
}

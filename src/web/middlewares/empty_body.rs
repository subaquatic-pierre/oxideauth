use axum::{
    body::{Body, Bytes},
    http::Request,
    middleware::Next,
    response::Response,
};
use tracing::debug;

/// Middleware that replaces an empty or whitespace-only request body with `{}`
/// so that endpoints accepting optional JSON bodies (like list endpoints)
/// don't fail with "Malformed JSON syntax" when the body is omitted.
///
/// This is applied at the router level, so it affects all downstream routes.
/// For endpoints with required fields (like create), sending no body will still
/// produce a meaningful validation error from serde rather than a bare JSON syntax error.
pub async fn empty_body_fallback(req: Request<Body>, next: Next) -> Response {
    // Buffer the entire body so we can inspect it
    let (parts, body) = req.into_parts();

    // 1 MB should be more than enough — typical JSON bodies are much smaller
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(err) => {
            // If we can't buffer the body (unlikely), pass through untouched
            debug!("empty_body_fallback: failed to buffer body: {err}");
            let req = Request::from_parts(parts, Body::from(Bytes::new()));
            return next.run(req).await;
        }
    };

    // If the body is empty or contains only whitespace, inject "{}"
    if bytes.is_empty() || bytes.iter().all(u8::is_ascii_whitespace) {
        debug!("empty_body_fallback: injecting '{{}}' for empty body");
        let new_body = Body::from("{}");
        let req = Request::from_parts(parts, new_body);
        return next.run(req).await;
    }

    // Otherwise, reconstruct the original body and continue
    let req = Request::from_parts(parts, Body::from(bytes));
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::middleware;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    async fn echo_body(body: Bytes) -> Bytes {
        body
    }

    fn test_app() -> Router {
        Router::new()
            .route("/echo", post(echo_body))
            .layer(middleware::from_fn(empty_body_fallback))
    }

    async fn send(app: Router, body: &str) -> (StatusCode, Vec<u8>) {
        let req = Request::builder()
            .method("POST")
            .uri("/echo")
            .body(Body::from(body.to_string()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 4096).await.unwrap().to_vec();
        (status, bytes)
    }

    #[tokio::test]
    async fn test_empty_body_is_injected_as_json_object() {
        let (status, body) = send(test_app(), "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"{}");
    }

    #[tokio::test]
    async fn test_whitespace_body_is_injected_as_json_object() {
        let (status, body) = send(test_app(), "   \n\t  ").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"{}");
    }

    #[tokio::test]
    async fn test_non_empty_json_body_passes_through() {
        let (status, body) = send(test_app(), r#"{"a":1}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, br#"{"a":1}"#);
    }

    #[tokio::test]
    async fn test_text_body_passes_through() {
        let (status, body) = send(test_app(), "hello").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"hello");
    }
}

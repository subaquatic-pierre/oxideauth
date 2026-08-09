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

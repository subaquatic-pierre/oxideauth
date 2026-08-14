use axum::http::Method;
use tower_http::cors::{Any, CorsLayer}; // Be sure to import http::Method

pub fn build_cors() -> CorsLayer {
    // Configure your CORS policy
    let cors_layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST]) // JSON-RPC is always POST
        .allow_origin(Any)
        .allow_headers(Any);

    cors_layer
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::header;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn cors_app() -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(build_cors())
    }

    #[tokio::test]
    async fn test_cors_preflight_allows_configured_methods() {
        let app = cors_app();
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/")
            .header(header::ORIGIN, "https://example.com")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        assert_eq!(
            res.headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
        let allow_methods = res
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_METHODS)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(allow_methods.contains("GET"), "methods: {allow_methods}");
        assert!(allow_methods.contains("POST"), "methods: {allow_methods}");
    }

    #[tokio::test]
    async fn test_cors_actual_request_includes_allow_origin() {
        let app = cors_app();
        let req = Request::builder()
            .method("GET")
            .uri("/")
            .header(header::ORIGIN, "https://example.com")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
    }
}

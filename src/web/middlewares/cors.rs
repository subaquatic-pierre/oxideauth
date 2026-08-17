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


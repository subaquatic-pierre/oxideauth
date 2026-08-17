use axum::{
    error_handling::HandleErrorLayer,
    extract::rejection::{JsonRejection, QueryRejection},
    http::StatusCode,
    response::IntoResponse,
    BoxError, Json,
};

use std::time::Duration;
use tower::{layer::util::Stack, Layer, Service, ServiceBuilder};
use tracing::error;

use crate::web::error::{ErrorBody, WebError};

pub struct FallbackMw;

impl FallbackMw {
    pub async fn global_error_handler(err: BoxError) -> impl IntoResponse {
        error!("Global error: {err}");

        // Try to downcast the BoxError into known types
        if let Some(web_err) = err.downcast_ref::<WebError>() {
            return web_err.clone().into_response();
        }

        // If it's a JSON body parse or extractor rejection, map generically
        let (status, msg) = if err.to_string().contains("JsonRejection") {
            (
                StatusCode::BAD_REQUEST,
                "Invalid or malformed JSON body.".to_string(),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected error occurred.".to_string(),
            )
        };

        let body = ErrorBody {
            success: false,
            status: status.as_u16(),
            message: msg,
        };

        (status, axum::Json(body)).into_response()
    }

    pub async fn fallback_handler() -> impl IntoResponse {
        let body = ErrorBody {
            success: false,
            status: StatusCode::NOT_FOUND.as_u16(),
            message: "The requested endpoint does not exist.".to_string(),
        };

        (StatusCode::NOT_FOUND, axum::Json(body))
    }
}

pub async fn global_error_handler(err: BoxError) -> impl IntoResponse {
    error!("Global error: {err}");

    // 1️⃣ If it's already a WebError, just convert to response
    if let Some(web_err) = err.downcast_ref::<WebError>() {
        return web_err.clone().into_response();
    }

    // 2️⃣ JSON body parse error
    if let Some(json_err) = err.downcast_ref::<JsonRejection>() {
        let msg = match json_err {
            JsonRejection::JsonDataError(e) => format!("Invalid JSON data: {e}"),
            JsonRejection::JsonSyntaxError(e) => format!("Malformed JSON syntax: {e}"),
            JsonRejection::MissingJsonContentType(_) => {
                "Missing `Content-Type: application/json` header".to_string()
            }
            _ => "Invalid JSON body".to_string(),
        };

        let body = ErrorBody {
            success: false,
            status: StatusCode::BAD_REQUEST.as_u16(),
            message: msg,
        };
        return (StatusCode::BAD_REQUEST, Json(body)).into_response();
    }

    // 5️⃣ Default fallback for unknown errors
    let body = ErrorBody {
        success: false,
        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        message: "An unexpected error occurred.".to_string(),
    };

    (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
}


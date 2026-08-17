use axum::extract::rejection::{FormRejection, JsonRejection, QueryRejection};
use axum::{
    // HTTP status codes and routing setup
    http::StatusCode,
    // Core response traits and JSON extractor
    response::{IntoResponse, Response},
    routing::get,
    Json,
    Router,
};
use derive_more::Display;
use serde::{Deserialize, Serialize};

use tracing::error;

use crate::store::error::StoreError;
use crate::{core::error::CoreError, web::response::WebResponse};

pub type JsonResResult<T> = Result<Json<T>, WebError>;
pub type JsonReqResult<T> = Result<Json<T>, JsonRejection>;

/// Defines specific, named errors that can occur in the application.
#[derive(Debug, Display, Clone)]
pub enum WebError {
    /// 404 Not Found error.
    NotFound,
    /// 500 Internal Server Error (use for unexpected failures).
    InternalServerError,
    /// 400 Bad Request error with a custom message.
    ValidationError(String),
    /// 409 Conflict error (e.g., uniqueness violations).
    Conflict(String),
    /// 401 Unauthorized error.
    Unauthorized,
    ReqStampNotInReqExt,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Indicates if the request was successful (always false for error responses).
    pub success: bool,
    /// The HTTP status code associated with the error.
    pub status: u16,
    /// A human-readable message describing the error.
    pub message: String,
}

/// Implementation of the IntoResponse trait for WebError.
/// This allows us to return `Err(WebError::...)` directly from a handler.
impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status_code, error_message) = match self {
            WebError::NotFound => (
                StatusCode::NOT_FOUND,
                "The requested resource was not found.".to_string(),
            ),
            WebError::InternalServerError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected server error occurred.".to_string(),
            ),
            WebError::ValidationError(msg) => (
                StatusCode::BAD_REQUEST,
                format!("Validation failed: {}", msg),
            ),
            WebError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            WebError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Authentication required or invalid credentials.".to_string(),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected server error occurred.".to_string(),
            ),
        };

        // Create the standardized error body
        let body = ErrorBody {
            success: false,
            status: status_code.as_u16(),
            message: error_message,
        };

        // Return the final response, combining the HTTP status code and the JSON error body
        (status_code, Json(body)).into_response()
    }
}

impl From<CoreError> for WebError {
    fn from(value: CoreError) -> Self {
        match value {
            // 400 Bad Request / Validation Errors
            CoreError::ParseError(msg) => WebError::ValidationError(msg),
            CoreError::InvalidParams(msg) => WebError::ValidationError(msg),
            CoreError::AlreadyExists(msg) => WebError::ValidationError(msg), // Conflict/Bad Request
            CoreError::EmailConflict(msg) => WebError::Conflict(msg),

            // 404 Not Found
            CoreError::StoreError(store_err) => match store_err {
                // 404 Not Found
                StoreError::EntityNotFound { entity, id } => WebError::NotFound,

                // 400 Bad Request / Validation (The primary change requested)
                StoreError::ListLimitExceeded { max, actual } => {
                    WebError::ValidationError(format!(
                        "Query limit exceeded: found {} items, max is {}",
                        actual, max
                    ))
                }
                // StoreError::DataError(msg) => WebError::ValidationError(msg),

                // // MAPPING SYSTEM/EXTERNAL ERRORS TO VALIDATION_ERROR (HTTP 400)
                // // This implies any failure in serialization, SQL query structure, or time parsing
                // // is due to user input data being malformed.

                // StoreError::BincodeError(err) => {
                //     WebError::ValidationError(format!("Bincode error: {}", err))
                // }
                // StoreError::FromHexError(err) => {
                //     WebError::ValidationError(format!("Hex conversion error: {}", err))
                // }
                // StoreError::SerdeJsonError(err) => {
                //     WebError::ValidationError(format!("JSON serialization error: {}", err))
                // }
                // StoreError::IntoSeaError(err) => {
                //     WebError::ValidationError(format!("Filter conversion error: {}", err))
                // }
                // StoreError::SqlxError(err) => {
                //     WebError::ValidationError(format!("Database query error: {}", err))
                // }
                // StoreError::SeaQueryError(err) => {
                //     WebError::ValidationError(format!("Query building error: {}", err))
                // }
                // StoreError::TimeParseError(err) => {
                //     WebError::ValidationError(format!("Time parsing error: {}", err))
                // }
                // StoreError::TimeFormatError(err) => {
                //     WebError::ValidationError(format!("Time formatting error: {}", err))
                // }

                // 500 Internal Server Errors (System/DB access issues that can't be attributed to bad input)
                StoreError::CantCreateDataStore(msg) => {
                    error!("Internal server error (store): {msg}");
                    WebError::InternalServerError
                },
                StoreError::WithTxnFalse | StoreError::NoTxn => {
                    error!("Internal server error (store): {store_err}");
                    WebError::InternalServerError
                },
                StoreError::MockReturn => {
                    error!("Internal server error (store): {store_err}");
                    WebError::InternalServerError
                },
                _ => WebError::ValidationError(format!("{}", store_err)),
            },

            // 401 Unauthorized
            CoreError::Auth(msg) => WebError::Unauthorized,
            CoreError::JsonWebTokenError(jwt_err) => WebError::Unauthorized,

            // 500 Internal Server Error (all other unexpected/critical errors)
            err => {
                error!("Internal server error: {err}");
                WebError::InternalServerError
            },
        }
    }
}

impl From<JsonRejection> for WebError {
    fn from(rej: JsonRejection) -> Self {
        match rej {
            JsonRejection::JsonSyntaxError(_) => {
                WebError::ValidationError("Malformed JSON syntax.".into())
            }
            JsonRejection::JsonDataError(_) => {
                WebError::ValidationError("Invalid JSON structure.".into())
            }
            JsonRejection::MissingJsonContentType(_) => {
                WebError::ValidationError("Missing Content-Type: application/json.".into())
            }
            _ => WebError::InternalServerError,
        }
    }
}

impl From<FormRejection> for WebError {
    fn from(rej: FormRejection) -> Self {
        match rej {
            FormRejection::InvalidFormContentType(_) => {
                WebError::ValidationError("Malformed JSON syntax.".into())
            }
            FormRejection::FailedToDeserializeForm(_) => {
                WebError::ValidationError("Invalid JSON structure.".into())
            }
            _ => WebError::InternalServerError,
        }
    }
}

impl From<QueryRejection> for WebError {
    fn from(rej: QueryRejection) -> Self {
        match rej {
            QueryRejection::FailedToDeserializeQueryString(_) => {
                WebError::ValidationError("Malformed JSON syntax.".into())
            }
            _ => WebError::InternalServerError,
        }
    }
}

impl std::error::Error for WebError {}


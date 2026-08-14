use axum::{
    Json,
    Router,
    // HTTP status codes and routing setup
    http::StatusCode,
    // Core response traits and JSON extractor
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::web::error::{JsonResResult, WebError};

#[derive(Debug, Serialize)]
pub struct WebResponse<T>
where
    T: Serialize,
{
    /// Indicates if the request was successful (always true for success responses).
    pub success: bool,
    /// The HTTP status code (redundant but useful for client debugging).
    pub status: u16,
    /// The actual data payload being returned.
    pub data: T,
}

impl<T: Serialize> WebResponse<T> {
    pub fn json(data: T) -> JsonResResult<Self> {
        let data = Self {
            success: true,
            status: StatusCode::OK.as_u16(),
            data,
        };

        Ok(Json(data))
    }

    pub fn json_flat(data: T) -> JsonResResult<T> {
        Ok(Json(data))
    }

    pub fn with_status(data: T, status: StatusCode) -> JsonResResult<Self> {
        Ok(Json(Self {
            success: true,
            status: status.as_u16(),
            data,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use axum::http::StatusCode;

    #[test]
    fn test_web_response_json_envelope() {
        let Json(resp) = WebResponse::json(vec![1, 2, 3]).expect("json should succeed");
        assert!(resp.success);
        assert_eq!(resp.status, StatusCode::OK.as_u16());
        assert_eq!(resp.data, vec![1, 2, 3]);

        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["status"], 200);
        assert_eq!(v["data"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_web_response_json_flat_returns_raw_data() {
        let Json(data) = WebResponse::json_flat("hello".to_string()).expect("json_flat should succeed");
        assert_eq!(data, "hello");
    }

    #[test]
    fn test_web_response_with_status() {
        let Json(resp) =
            WebResponse::with_status("created".to_string(), StatusCode::CREATED)
                .expect("with_status should succeed");
        assert!(resp.success);
        assert_eq!(resp.status, StatusCode::CREATED.as_u16());
        assert_eq!(resp.status, 201);
        assert_eq!(resp.data, "created");
    }

    #[test]
    fn test_web_response_serializes_object_payload() {
        let payload = serde_json::json!({ "key": "value", "n": 7 });
        let Json(resp) = WebResponse::json(payload.clone()).unwrap();
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["data"], payload);
    }
}

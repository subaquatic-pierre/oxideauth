use std::{error::Error, fmt::Display};

use derive_more::From;
use redis::RedisError;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

pub type CacheResult<T> = Result<T, CacheError>;

#[serde_as]
#[derive(Debug, Serialize, From)]
pub enum CacheError {
    Init(String),
    NotFound(String),
    InvalidSetOperation(String),
    ParseError(String),
    #[from]
    RedisError(#[serde_as(as = "DisplayFromStr")] redis::RedisError),
    #[from]
    SerdeError(#[serde_as(as = "DisplayFromStr")] serde_json::Error),
    #[from]
    StoreError(#[serde_as(as = "DisplayFromStr")] crate::store::error::StoreError),
}

impl Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}");
        Ok(())
    }
}

impl Error for CacheError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::error::StoreError;

    #[test]
    fn test_display_is_non_empty() {
        let err = CacheError::NotFound("x".into());
        assert!(!err.to_string().is_empty());
        assert!(err.to_string().contains("NotFound"));
    }

    #[test]
    fn test_from_serde_json_error() {
        let serde_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = CacheError::from(serde_err);
        assert!(matches!(err, CacheError::SerdeError(_)));
    }

    #[test]
    fn test_from_redis_error() {
        let redis_err = redis::RedisError::from((redis::ErrorKind::IoError, "test"));
        let err = CacheError::from(redis_err);
        assert!(matches!(err, CacheError::RedisError(_)));
    }

    #[test]
    fn test_from_store_error() {
        let store_err = StoreError::MockReturn;
        let err = CacheError::from(store_err);
        assert!(matches!(err, CacheError::StoreError(_)));
    }
}

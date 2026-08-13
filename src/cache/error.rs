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

use std::{convert::Infallible, fmt::Display, num::ParseIntError, str::ParseBoolError};

use derive_more::From;
use serde::{Deserialize, Serialize};
use serde_json::Error as JsonError;
use serde_with::{DisplayFromStr, serde_as};
use time::error::{Format, Parse};

pub type CoreResult<T> = core::result::Result<T, CoreError>;

#[serde_as]
#[derive(Debug, Serialize, From)]
pub enum CoreError {
    ApiError(String),
    AlreadyExists(String),
    ParseError(String),
    Auth(String),
    InvalidParams(String),
    NotFound(String),

    #[from]
    UuidError(#[serde_as(as = "DisplayFromStr")] uuid::Error),
    #[from]
    ReqwestError(#[serde_as(as = "DisplayFromStr")] reqwest::Error),
    #[from]
    StoreError(#[serde_as(as = "DisplayFromStr")] crate::store::error::StoreError),
    #[from]
    CacheError(#[serde_as(as = "DisplayFromStr")] crate::cache::error::CacheError),
    #[from]
    JsonWebTokenError(#[serde_as(as = "DisplayFromStr")] jsonwebtoken::errors::Error),
    #[from]
    FormatError(#[serde_as(as = "DisplayFromStr")] std::fmt::Error),
    #[from]
    BincodeError(#[serde_as(as = "DisplayFromStr")] bincode::Error),
    #[from]
    InfallibleError(#[serde_as(as = "DisplayFromStr")] Infallible),
}

impl Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}");
        Ok(())
    }
}

impl std::error::Error for CoreError {}

impl From<ParseBoolError> for CoreError {
    fn from(value: ParseBoolError) -> Self {
        CoreError::ParseError(format!("error parsing bool, {value}"))
    }
}

impl From<ParseIntError> for CoreError {
    fn from(value: ParseIntError) -> Self {
        CoreError::ParseError(format!("error parsing int, {value}"))
    }
}

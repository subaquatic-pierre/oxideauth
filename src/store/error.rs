use std::fmt::Display;

use derive_more::From;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

pub type StoreResult<T> = core::result::Result<T, StoreError>;

#[serde_as]
#[derive(Debug, Serialize, From)]
pub enum StoreError {
    // --- Query Errors
    EntityNotFound {
        entity: String,
        id: String,
    },
    ListLimitExceeded {
        max: i64,
        actual: i64,
    },
    DataError(String),

    // --- StoreManager
    CantCreateDataStore(String),

    // --- DBx Errors
    WithTxnFalse,
    NoTxn,

    MockReturn,

    // --- Externals
    #[from]
    BincodeError(#[serde_as(as = "DisplayFromStr")] bincode::Error),
    #[from]
    FromHexError(#[serde_as(as = "DisplayFromStr")] hex::FromHexError),
    #[from]
    SerdeJsonError(#[serde_as(as = "DisplayFromStr")] serde_json::Error),
    #[from]
    IntoSeaError(#[serde_as(as = "DisplayFromStr")] modql::filter::IntoSeaError),
    #[from]
    SqlxError(#[serde_as(as = "DisplayFromStr")] sqlx::Error),
    #[from]
    SeaQueryError(#[serde_as(as = "DisplayFromStr")] sea_query::error::Error),
    #[from]
    TimeParseError(#[serde_as(as = "DisplayFromStr")] time::error::Parse),
    #[from]
    TimeFormatError(#[serde_as(as = "DisplayFromStr")] time::error::Format),
}

impl Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for StoreError {}

use std::fmt::Display;

use derive_more::From;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

pub type StoreResult<T> = core::result::Result<T, StoreError>;

#[serde_as]
#[derive(Debug, Serialize, From)]
pub enum StoreError {
    // --- Query Errors
    EntityNotFound {
        entity: String,
        id: String,
    },
    InvalidContext(String),
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
    SeaQueryError(#[serde_as(as = "DisplayFromStr")] sea_query::error::Error),
    #[from]
    TimeParseError(#[serde_as(as = "DisplayFromStr")] time::error::Parse),
    #[from]
    TimeFormatError(#[serde_as(as = "DisplayFromStr")] time::error::Format),
    SqlxError(#[serde_as(as = "DisplayFromStr")] sqlx::Error),
    ConstraintViolation,
}

impl From<sqlx::Error> for StoreError {
    fn from(value: sqlx::Error) -> Self {
        match value {
            sqlx::Error::Database(ref database_error) => {
                // Constraint violations that services handle uniformly by
                // matching on `StoreError::ConstraintViolation`:
                //   - 23503 foreign_key_violation: the row is still referenced
                //     by a foreign key (e.g. deleting a permission still
                //     attached to a role).
                //   - 23505 unique_violation: a value duplicates one already
                //     present under a unique constraint (e.g. renaming a
                //     permission to a name already used in the same workspace).
                match database_error.code().as_deref() {
                    Some("23503" | "23505") => Self::ConstraintViolation,
                    _ => Self::SqlxError(value),
                }
            }
            _ => Self::SqlxError(value),
        }
    }
}

impl Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for StoreError {}


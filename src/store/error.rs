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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_is_non_empty() {
        // -- Setup
        let cases = [
            StoreError::EntityNotFound {
                entity: "account".to_string(),
                id: "abc".to_string(),
            },
            StoreError::ListLimitExceeded { max: 500, actual: 501 },
            StoreError::DataError("boom".to_string()),
            StoreError::MockReturn,
            StoreError::WithTxnFalse,
            StoreError::NoTxn,
        ];

        // -- Execute & Assert
        for err in cases {
            let s = err.to_string();
            assert!(!s.is_empty(), "Display output must be non-empty");
        }
    }

    #[test]
    fn test_from_external_errors() {
        // -- Execute
        let serde_err: StoreError =
            serde_json::from_str::<serde_json::Value>("{invalid json}").unwrap_err().into();
        let parse_res = time::OffsetDateTime::parse(
            "not-a-time",
            &time::format_description::well_known::Rfc3339,
        );
        let time_err: StoreError = parse_res.unwrap_err().into();

        // -- Assert
        assert!(!serde_err.to_string().is_empty());
        assert!(!time_err.to_string().is_empty());
        assert!(matches!(serde_err, StoreError::SerdeJsonError(_)));
        assert!(matches!(time_err, StoreError::TimeParseError(_)));
    }
}

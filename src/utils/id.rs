use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    core::error::{CoreError, CoreResult},
    store::entities::{
        audit::{AuditFields, AuditMeta},
        id::DbId,
    },
};

// Helper to convert DbId to Uuid for Option fields
pub fn map_optional_db_id(db_id: Option<DbId>) -> Option<Uuid> {
    db_id.map(|id| id.into())
}

pub fn id_or_string(
    id: Option<Uuid>,
    fallback: Option<String>,
    msg: Option<&str>,
) -> CoreResult<String> {
    let res = id
        .map(|id| id.to_string())
        .or(fallback.clone())
        .ok_or(CoreError::InvalidParams(
            msg.unwrap_or("ID or identifier required").to_string(),
        ));

    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::CoreError;

    #[test]
    fn test_map_optional_db_id_some() {
        let uuid = Uuid::new_v4();
        assert_eq!(map_optional_db_id(Some(DbId(uuid))), Some(uuid));
    }

    #[test]
    fn test_map_optional_db_id_none() {
        assert_eq!(map_optional_db_id(None), None);
    }

    #[test]
    fn test_id_or_string_prefers_id_over_fallback() {
        let uuid = Uuid::new_v4();
        let res = id_or_string(Some(uuid), Some("fallback".to_string()), None).unwrap();
        assert_eq!(res, uuid.to_string());
    }

    #[test]
    fn test_id_or_string_uses_fallback_when_id_none() {
        let res = id_or_string(None, Some("some-slug".to_string()), None).unwrap();
        assert_eq!(res, "some-slug");
    }

    #[test]
    fn test_id_or_string_errors_when_both_none() {
        let err = id_or_string(None, None, None).unwrap_err();
        match err {
            CoreError::InvalidParams(msg) => {
                assert_eq!(msg, "ID or identifier required");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn test_id_or_string_respects_custom_message() {
        let err = id_or_string(None, None, Some("ID or slug required")).unwrap_err();
        match err {
            CoreError::InvalidParams(msg) => {
                assert_eq!(msg, "ID or slug required");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}

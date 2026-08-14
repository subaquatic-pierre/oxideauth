use modql::filter::SeaResult;
use sea_query::Value as SeaValue;
use serde::Serialize;
use serde_json::{to_value, Value as JsonValue};
use std::fmt::Debug;
use time::{format_description::well_known::Rfc3339, serde::rfc3339, OffsetDateTime};

use crate::store::error::{StoreError, StoreResult};

pub fn time_to_sea_value(json_value: JsonValue) -> SeaResult<SeaValue> {
    Ok(rfc3339::deserialize(json_value)?.into())
}

pub fn try_time_to_string(time: OffsetDateTime) -> StoreResult<String> {
    time.format(&Rfc3339)
        .map_err(|e| StoreError::TimeFormatError(e))
}

pub fn time_to_string(time: OffsetDateTime) -> String {
    try_time_to_string(time).unwrap()
}

pub fn try_time_from_string(time: &str) -> StoreResult<OffsetDateTime> {
    OffsetDateTime::parse(time, &Rfc3339).map_err(|e| StoreError::TimeParseError(e))
}

pub fn time_from_string(time: &str) -> OffsetDateTime {
    try_time_from_string(time).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::error::StoreError;
    use sea_query::Value as SeaValue;
    use serde_json::json;
    use time::OffsetDateTime;

    #[test]
    fn test_try_time_to_string_rfc3339() {
        // -- Setup
        let epoch = OffsetDateTime::UNIX_EPOCH;

        // -- Execute
        let s = try_time_to_string(epoch).unwrap();

        // -- Assert
        assert_eq!(s, "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_time_to_string_matches_rfc3339() {
        // -- Setup
        let t = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();

        // -- Execute
        let s = time_to_string(t);

        // -- Assert
        assert_eq!(s, t.format(&Rfc3339).unwrap());
    }

    #[test]
    fn test_try_time_from_string_ok() {
        // -- Execute
        let t = try_time_from_string("1970-01-01T00:00:00Z").unwrap();

        // -- Assert
        assert_eq!(t, OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn test_time_from_string_roundtrip() {
        // -- Setup
        let original = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();

        // -- Execute
        let s = time_to_string(original);
        let back = time_from_string(&s);

        // -- Assert
        assert_eq!(back, original);
    }

    #[test]
    fn test_try_time_from_string_err() {
        // -- Execute
        let res = try_time_from_string("not-a-timestamp");

        // -- Assert
        assert!(matches!(res, Err(StoreError::TimeParseError(_))));
    }

    #[test]
    fn test_time_to_sea_value() {
        // -- Setup
        let epoch_json = json!("1970-01-01T00:00:00Z");

        // -- Execute
        let v = time_to_sea_value(epoch_json).unwrap();

        // -- Assert
        assert_eq!(
            v,
            SeaValue::TimeDateTimeWithTimeZone(Some(OffsetDateTime::UNIX_EPOCH))
        );
    }
}

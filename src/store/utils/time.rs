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

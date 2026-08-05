use modql::filter::SeaResult;
use sea_query::Value as SeaValue;
use serde::Serialize;
use serde_json::{to_value, Value as JsonValue};
use std::fmt::Debug;
use time::{format_description::well_known::Rfc3339, serde::rfc3339, OffsetDateTime};

use crate::store::error::StoreError;

pub fn json_to_sea_value(v: JsonValue) -> SeaResult<SeaValue> {
    match serde_json::to_value(v) {
        Ok(v) => Ok(SeaValue::Json(Some(Box::new(v)))),
        Err(e) => {
            tracing::error!(?e, "failed to serialize meta");
            Err(e.into())
        }
    }
}

pub fn bytes_to_sea_value(bytes: &[u8]) -> SeaValue {
    SeaValue::Bytes(Some(bytes.to_vec().into()))
}

pub fn to_sea_bool(val: JsonValue) -> SeaResult<SeaValue> {
    match val {
        JsonValue::Bool(v) => Ok(SeaValue::Bool(Some(v))),
        _ => Err(modql::filter::IntoSeaError::Custom(
            "invalid bool type".to_string(),
        )),
    }
}

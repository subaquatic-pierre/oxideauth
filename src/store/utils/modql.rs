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
    SeaValue::Bytes(Some(bytes.to_vec()))
}

pub fn to_sea_bool(val: JsonValue) -> SeaResult<SeaValue> {
    match val {
        JsonValue::Bool(v) => Ok(SeaValue::Bool(Some(v))),
        _ => Err(modql::filter::IntoSeaError::Custom(
            "invalid bool type".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modql::filter::IntoSeaError;
    use sea_query::Value as SeaValue;
    use serde_json::json;

    #[test]
    fn test_json_to_sea_value() {
        // -- Setup
        let value = json!({"schema_version": "1", "tags": ["a", "b"]});

        // -- Execute
        let v = json_to_sea_value(value.clone()).unwrap();

        // -- Assert
        match v {
            SeaValue::Json(Some(boxed)) => assert_eq!(*boxed, value),
            other => panic!("expected SeaValue::Json(Some(_)), got {other:?}"),
        }
    }

    #[test]
    fn test_bytes_to_sea_value() {
        // -- Setup
        let bytes = vec![1u8, 2, 3, 4];

        // -- Execute
        let v = bytes_to_sea_value(&bytes);

        // -- Assert
        assert_eq!(v, SeaValue::Bytes(Some(bytes)));
    }

    #[test]
    fn test_to_sea_bool_valid() {
        // -- Execute
        let true_val = to_sea_bool(json!(true)).unwrap();
        let false_val = to_sea_bool(json!(false)).unwrap();

        // -- Assert
        assert_eq!(true_val, SeaValue::Bool(Some(true)));
        assert_eq!(false_val, SeaValue::Bool(Some(false)));
    }

    #[test]
    fn test_to_sea_bool_invalid() {
        // -- Execute
        let res = to_sea_bool(json!("yes"));
        let res_num = to_sea_bool(json!(42));

        // -- Assert
        assert!(matches!(res, Err(IntoSeaError::Custom(_))));
        assert!(matches!(res_num, Err(IntoSeaError::Custom(_))));
    }
}

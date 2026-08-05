use modql::field::HasSeaFields;
use sea_query::{ArrayType, Iden, Value as SeaValue}; // for .to_string() on SeaField/Iden
use sqlx::{postgres::PgRow, FromRow, Postgres, QueryBuilder};
use uuid::Uuid;

// Bind a sea_query::Value into QueryBuilder with correct SQL type where possible.
// For None/nulls we push literal NULL (no binding).
pub fn push_sq_value(qb: &mut QueryBuilder<Postgres>, v: &SeaValue) {
    match v {
        SeaValue::Bool(Some(x)) => {
            qb.push_bind(*x);
        }
        SeaValue::Int(Some(x)) => {
            qb.push_bind(*x);
        }
        SeaValue::BigInt(Some(x)) => {
            qb.push_bind(*x);
        }
        SeaValue::Unsigned(Some(x)) => {
            qb.push_bind(*x as i64);
        } // map to i64 if needed
        SeaValue::BigUnsigned(Some(x)) => {
            qb.push_bind(*x as i64);
        }
        SeaValue::Float(Some(x)) => {
            qb.push_bind(*x);
        }
        SeaValue::Double(Some(x)) => {
            qb.push_bind(*x);
        }
        SeaValue::String(Some(s)) => {
            qb.push_bind(s.to_string());
        }
        SeaValue::Char(Some(c)) => {
            let mut s = String::new();
            s.push(*c);
            qb.push_bind(s);
        }
        SeaValue::Uuid(Some(u)) => {
            qb.push_bind(**u);
        }
        SeaValue::Json(Some(j)) => {
            qb.push_bind(sqlx::types::Json(j.clone()));
        }
        // Bytes
        SeaValue::Bytes(Some(b)) => {
            qb.push_bind(*b.clone());
        }
        SeaValue::TimeDateTimeWithTimeZone(Some(t)) => {
            qb.push_bind(**t);
        }
        SeaValue::Array(ArrayType::String, Some(items)) => {
            let arr: Vec<Option<String>> = items
                .iter()
                .map(|v| match v {
                    SeaValue::String(Some(s)) => Some(s.to_string()),
                    SeaValue::String(None) => None,
                    _ => None,
                })
                .collect();
            qb.push_bind(arr);
        }

        // Anything else or NULL → literal NULL
        _ => {
            qb.push("NULL");
        }
    }
}

pub fn pg_type_of(v: &sea_query::Value) -> &'static str {
    match v {
        SeaValue::String(_) => "text",
        SeaValue::Bool(_) => "bool",
        SeaValue::Int(_) | SeaValue::SmallInt(_) | SeaValue::TinyInt(_) => "int4",
        SeaValue::BigInt(_) => "int8",
        SeaValue::Uuid(_) => "uuid",
        SeaValue::Json(_) => "jsonb",
        SeaValue::TimeDateTimeWithTimeZone(_) => "timestamptz",
        SeaValue::TimeDateTime(_) => "time",
        SeaValue::Array(ArrayType::String, _) => "text[]",
        // add others you use…
        _ => "", // safe fallback if you truly don’t know
    }
}

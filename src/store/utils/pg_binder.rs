//! Postgres value binder for `sea-query`.
//!
//! This module is a drop-in replacement for the `sea-query-binder` crate's
//! `sqlx-postgres` feature. It converts a `Vec<sea_query::Value>` into a
//! `sqlx::postgres::PgArguments` so that `sea-query` generated statements can
//! be executed directly via `sqlx::query_with`.
//!
//! Only the features actually used by this project are implemented:
//!
//! - `with-uuid`
//! - `with-time`
//! - `with-json`
//! - `postgres-array` (all array type variants)
//!
//! Everything else (`Enum`, chrono, bigdecimal, rust_decimal, ipnetwork,
//! mac_address, postgres-vector, ...) is intentionally ignored via a
//! `_ => {}` catch-all.

use serde_json::Value as Json;
use uuid::Uuid;

use sea_query::{ArrayType, Value};

/// Wrapper around `sea-query` values that implements
/// [`sqlx::IntoArguments`] for `sqlx::postgres::Postgres`.
#[derive(Debug)]
pub struct PgBinder(pub Vec<Value>);

impl sqlx::IntoArguments<'_, sqlx::postgres::Postgres> for PgBinder {
    fn into_arguments(self) -> sqlx::postgres::PgArguments {
        let mut args = sqlx::postgres::PgArguments::default();
        for arg in self.0.into_iter() {
            use sqlx::Arguments;
            match arg {
                Value::Bool(b) => {
                    let _ = args.add(b);
                }
                Value::TinyInt(i) => {
                    let _ = args.add(i);
                }
                Value::SmallInt(i) => {
                    let _ = args.add(i);
                }
                Value::Int(i) => {
                    let _ = args.add(i);
                }
                Value::BigInt(i) => {
                    let _ = args.add(i);
                }
                Value::TinyUnsigned(i) => {
                    let _ = args.add(i.map(|i| i as i16));
                }
                Value::SmallUnsigned(i) => {
                    let _ = args.add(i.map(|i| i as i32));
                }
                Value::Unsigned(i) => {
                    let _ = args.add(i.map(|i| i as i64));
                }
                Value::BigUnsigned(i) => {
                    let _ = args.add(i.map(|i| <i64 as TryFrom<u64>>::try_from(i).unwrap()));
                }
                Value::Float(f) => {
                    let _ = args.add(f);
                }
                Value::Double(d) => {
                    let _ = args.add(d);
                }
                Value::String(s) => {
                    let _ = args.add(s.as_deref());
                }
                Value::Char(c) => {
                    let _ = args.add(c.map(|c| c.to_string()));
                }
                Value::Bytes(b) => {
                    let _ = args.add(b.as_deref());
                }
                Value::TimeDate(t) => {
                    let _ = args.add(t);
                }
                Value::TimeTime(t) => {
                    let _ = args.add(t);
                }
                Value::TimeDateTime(t) => {
                    let _ = args.add(t);
                }
                Value::TimeDateTimeWithTimeZone(t) => {
                    let _ = args.add(t);
                }
                Value::Uuid(uuid) => {
                    let _ = args.add(uuid);
                }
                Value::Json(j) => {
                    let _ = args.add(j.map(|v| *v));
                }
                Value::Array(ty, v) => match ty {
                    ArrayType::Bool => {
                        let value: Option<Vec<bool>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Bool");
                        let _ = args.add(value);
                    }
                    ArrayType::TinyInt => {
                        let value: Option<Vec<i8>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::TinyInt");
                        let _ = args.add(value);
                    }
                    ArrayType::SmallInt => {
                        let value: Option<Vec<i16>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::SmallInt");
                        let _ = args.add(value);
                    }
                    ArrayType::Int => {
                        let value: Option<Vec<i32>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Int");
                        let _ = args.add(value);
                    }
                    ArrayType::BigInt => {
                        let value: Option<Vec<i64>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::BigInt");
                        let _ = args.add(value);
                    }
                    ArrayType::TinyUnsigned => {
                        let value: Option<Vec<u8>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::TinyUnsigned");
                        let value: Option<Vec<i16>> =
                            value.map(|vec| vec.into_iter().map(|i| i as i16).collect());
                        let _ = args.add(value);
                    }
                    ArrayType::SmallUnsigned => {
                        let value: Option<Vec<u16>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::SmallUnsigned");
                        let value: Option<Vec<i32>> =
                            value.map(|vec| vec.into_iter().map(|i| i as i32).collect());
                        let _ = args.add(value);
                    }
                    ArrayType::Unsigned => {
                        let value: Option<Vec<u32>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Unsigned");
                        let value: Option<Vec<i64>> =
                            value.map(|vec| vec.into_iter().map(|i| i as i64).collect());
                        let _ = args.add(value);
                    }
                    ArrayType::BigUnsigned => {
                        let value: Option<Vec<u64>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::BigUnsigned");
                        let value: Option<Vec<i64>> = value.map(|vec| {
                            vec.into_iter()
                                .map(|i| <i64 as TryFrom<u64>>::try_from(i).unwrap())
                                .collect()
                        });
                        let _ = args.add(value);
                    }
                    ArrayType::Float => {
                        let value: Option<Vec<f32>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Float");
                        let _ = args.add(value);
                    }
                    ArrayType::Double => {
                        let value: Option<Vec<f64>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Double");
                        let _ = args.add(value);
                    }
                    ArrayType::String => {
                        let value: Option<Vec<String>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::String");
                        let _ = args.add(value);
                    }
                    ArrayType::Char => {
                        let value: Option<Vec<char>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Char");
                        let value: Option<Vec<String>> =
                            value.map(|vec| vec.into_iter().map(|c| c.to_string()).collect());
                        let _ = args.add(value);
                    }
                    ArrayType::Bytes => {
                        let value: Option<Vec<Vec<u8>>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Bytes");
                        let _ = args.add(value);
                    }
                    ArrayType::TimeDate => {
                        let value: Option<Vec<time::Date>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::TimeDate");
                        let _ = args.add(value);
                    }
                    ArrayType::TimeTime => {
                        let value: Option<Vec<time::Time>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::TimeTime");
                        let _ = args.add(value);
                    }
                    ArrayType::TimeDateTime => {
                        let value: Option<Vec<time::PrimitiveDateTime>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::TimeDateTime");
                        let _ = args.add(value);
                    }
                    ArrayType::TimeDateTimeWithTimeZone => {
                        let value: Option<Vec<time::OffsetDateTime>> = Value::Array(ty, v).expect(
                            "This Value::Array should consist of Value::TimeDateTimeWithTimeZone",
                        );
                        let _ = args.add(value);
                    }
                    ArrayType::Uuid => {
                        let value: Option<Vec<Uuid>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Uuid");
                        let _ = args.add(value);
                    }
                    ArrayType::Json => {
                        let value: Option<Vec<Json>> = Value::Array(ty, v)
                            .expect("This Value::Array should consist of Value::Json");
                        let _ = args.add(value);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        args
    }
}

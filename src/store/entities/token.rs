use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::id::DbId;
use crate::store::traits::meta::HasId;
use oxideauth_macros::{EnumTextType, HasId};

use crate::store::entities::audit::{AuditFields, AuditMeta};
use crate::store::entities::hash::Sha256Hash;
use crate::store::error::{StoreError, StoreResult};
use crate::store::utils::{bytes_to_sea_value, json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]
pub enum TokenIden {
    #[iden = "token"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
}

// --- Row (DB-facing) ---
/// Maps to the `token` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct TokenRow {
    pub id: DbId,
    pub hash: Sha256Hash,
    pub kind: TokenKind,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub expires_at: OffsetDateTime,
    pub reason: Option<String>,
    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: TokenMeta,
    #[sqlx(flatten)]
    pub audit: AuditFields,
}

#[derive(Debug, Serialize, Deserialize, Clone, EnumTextType)]
pub enum TokenKind {
    #[serde(rename = "auth")]
    Auth,
    #[serde(rename = "password_reset")]
    PasswordReset,
}

impl From<TokenKind> for SeaValue {
    fn from(value: TokenKind) -> Self {
        let s = format!("{value}");
        SeaValue::String(Some(Box::new(s)))
    }
}

impl Nullable for TokenKind {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

// --- Create (store input) ---
/// Input for creating a new `token` entry.
#[derive(Debug, Fields)]
pub struct TokenForCreate {
    pub hash: Sha256Hash,
    pub account_id: Uuid,
    pub kind: TokenKind,
    pub workspace_id: Uuid,
    pub expires_at: OffsetDateTime,
    pub reason: Option<String>,
    pub tags: Vec<String>,
    pub meta: TokenMeta,
}

// --- Update (store input) ---
/// Input for updating an existing `token` entry.
#[derive(Debug, Fields, Clone)]
pub struct TokenForUpdate {
    pub account_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub expires_at: Option<OffsetDateTime>,
    pub reason: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<TokenMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct TokenMeta {
    pub schema_version: String,
}

impl Nullable for TokenMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<TokenMeta> for SeaValue {
    fn from(value: TokenMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `token` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct TokenFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    pub hash: Option<Sha256Hash>,
    #[modql(cast_as = "uuid")]
    pub account_id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub workspace_id: Option<OpValsString>,
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub expires_at: Option<OpValsValue>,
    pub reason: Option<OpValsString>,
    pub kind: Option<OpValsString>,

    // Audit filters
    #[modql(cast_as = "uuid")]
    pub created_by: Option<OpValsString>,
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub created_at: Option<OpValsValue>,
    #[modql(cast_as = "uuid")]
    pub updated_by: Option<OpValsString>,
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub updated_at: Option<OpValsValue>,
}

impl TryFrom<JsonValue> for TokenFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

// --- Defaults for testing ---
#[cfg(test)]
impl Default for TokenForCreate {
    fn default() -> Self {
        Self {
            hash: Sha256Hash::gen_rand(),
            account_id: Uuid::default(),
            kind: TokenKind::Auth,
            workspace_id: Uuid::default(),
            expires_at: OffsetDateTime::now_utc(),
            reason: Some("test_revoke".into()),
            tags: vec![],
            meta: TokenMeta {
                schema_version: "1".into(),
            },
        }
    }
}

#[cfg(test)]
impl Default for TokenForUpdate {
    fn default() -> Self {
        Self {
            account_id: None,
            workspace_id: None,
            expires_at: None,
            reason: None,
            tags: None,
            meta: None,
        }
    }
}

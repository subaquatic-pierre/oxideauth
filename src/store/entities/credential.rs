use std::str::FromStr;

use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use oxideauth_macros::{EnumTextType, HasId};
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::prelude::FromRow;
use strum_macros::{Display, EnumString};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::audit::{AuditFields, AuditMeta};
use crate::store::entities::id::DbId;
use crate::store::error::{StoreError, StoreResult};
use crate::store::traits::meta::HasId;
use crate::store::utils::{gen_rand_str, json_to_sea_value, time_to_sea_value};

pub const DEFAULT_JWT_MAX_AGE: u64 = 604800;

#[derive(Iden, Copy, Clone)]
pub enum CredentialIden {
    #[iden = "credential"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    Tags,
    Meta,
}

// --- Row (DB-facing) ---
/// Maps to the `credential` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct CredentialRow {
    pub id: DbId,

    pub account_id: DbId,
    pub workspace_id: DbId,
    pub membership_id: Option<DbId>,
    pub kind: CredentialKind,
    pub provider: CredentialProvider,
    pub status: CredentialStatus,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    pub secret: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
    pub last_used_at: Option<OffsetDateTime>,
    #[sqlx(json)]
    pub config: CredentialConfig,
    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: CredentialMeta,
    #[sqlx(flatten)]
    pub audit: AuditFields,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, EnumTextType)]
#[serde(rename_all = "lowercase")]
pub enum CredentialStatus {
    Active,
    Revoked,
    Pending,
    Disabled,
}

impl From<CredentialStatus> for SeaValue {
    fn from(value: CredentialStatus) -> Self {
        let s = format!("{value}");
        SeaValue::String(Some(s))
    }
}

impl Nullable for CredentialStatus {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, EnumTextType)]
#[serde(rename_all = "lowercase")]
pub enum CredentialProvider {
    Local,
    Google,
    Github,
}

impl From<CredentialProvider> for SeaValue {
    fn from(value: CredentialProvider) -> Self {
        let s = format!("{value}");
        SeaValue::String(Some(s))
    }
}

impl Nullable for CredentialProvider {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, EnumTextType)]
pub enum CredentialKind {
    #[serde(rename = "password")]
    Password,
    #[serde(rename = "oauth")]
    OAuth,
    #[serde(rename = "sso")]
    SSO,
    #[serde(rename = "api_key")]
    ApiKey,
}

impl From<CredentialKind> for SeaValue {
    fn from(value: CredentialKind) -> Self {
        let s = format!("{value}");
        SeaValue::String(Some(s))
    }
}

impl Nullable for CredentialKind {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

#[derive(Debug, Fields)]
pub struct CredentialForCreate {
    pub kind: CredentialKind,
    pub provider: CredentialProvider,
    pub status: CredentialStatus,
    pub account_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub membership_id: Option<Uuid>,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    pub secret: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
    pub last_used_at: Option<OffsetDateTime>,
    pub config: CredentialConfig,
    pub tags: Vec<String>,
    pub meta: CredentialMeta,
}

#[derive(Debug, Fields, Clone)]
pub struct CredentialForUpdate {
    pub kind: Option<CredentialKind>,
    pub provider: Option<CredentialProvider>,
    pub status: Option<CredentialStatus>,
    pub membership_id: Option<Uuid>,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    pub secret: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
    pub config: Option<CredentialConfig>,
    pub last_used_at: Option<OffsetDateTime>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<CredentialMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct CredentialMeta {
    pub schema_version: String,
}

impl Nullable for CredentialMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<CredentialMeta> for SeaValue {
    fn from(value: CredentialMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `credential` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct CredentialFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub account_id: Option<OpValsString>,
    #[modql(cast_as = "uuid", rel = "credential")]
    pub workspace_id: Option<OpValsString>,
    pub kind: Option<OpValsString>,
    pub provider: Option<OpValsString>,
    pub secret: Option<OpValsString>,
    pub provider_id: Option<OpValsString>,
    pub email: Option<OpValsString>,
    pub status: Option<OpValsString>,

    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub last_used_at: Option<OpValsValue>,

    // Audit filters (created_by/at, updated_by/at)
    #[modql(cast_as = "uuid")]
    pub created_by: Option<OpValsString>,
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub created_at: Option<OpValsValue>,
    #[modql(cast_as = "uuid")]
    pub updated_by: Option<OpValsString>,
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub updated_at: Option<OpValsValue>,
}

impl TryFrom<JsonValue> for CredentialFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

#[derive(Debug, Fields, Serialize, Deserialize, Clone)]
pub struct CredentialConfig {
    jwt_max_age: u64,
}

impl Default for CredentialConfig {
    fn default() -> Self {
        Self {
            jwt_max_age: DEFAULT_JWT_MAX_AGE,
        }
    }
}

impl Nullable for CredentialConfig {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<CredentialConfig> for SeaValue {
    fn from(value: CredentialConfig) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

// --- Defaults for testing ---
#[cfg(any(test, feature = "integration"))]
impl Default for CredentialForCreate {
    fn default() -> Self {
        let email = format!("{}@{}.com", gen_rand_str(5), gen_rand_str(5));
        Self {
            account_id: Uuid::nil(),
            workspace_id: Some(Uuid::nil()),
            membership_id: None,
            kind: CredentialKind::Password,
            provider: CredentialProvider::Local,
            status: CredentialStatus::Active,
            provider_id: None,
            email: Some(email),
            secret: Some("hashed_password_placeholder".to_string()),
            expires_at: None,
            last_used_at: None,
            tags: vec![],
            config: CredentialConfig::default(),
            meta: CredentialMeta::default(),
        }
    }
}

#[cfg(test)]
impl Default for CredentialForUpdate {
    fn default() -> Self {
        Self {
            kind: None,
            provider: None,
            provider_id: None,
            email: None,
            secret: None,
            status: None,
            membership_id: None,
            expires_at: None,
            config: None,
            last_used_at: None,
            tags: None,
            meta: None,
        }
    }
}

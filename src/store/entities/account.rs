use crate::impl_has_active_filter;
use crate::store::filter;
use derive_more::Display;
use oxideauth_macros::HasId;
use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use sea_query::{sea_value_to_json_value, Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::audit::AuditFields;
use crate::store::error::{StoreError, StoreResult};

use crate::store::entities::credential::{
    CredentialKind, CredentialProvider, CredentialRow, CredentialStatus,
};
use crate::store::entities::id::DbId;
use crate::store::utils::{json_to_sea_value, time_to_sea_value, to_sea_bool};

use crate::store::traits::meta::HasId;

#[derive(Iden, Copy, Clone)]
pub enum AccountIden {
    #[iden = "account"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    AccountId,
    Credential,
    Credentials,
    Tags,
    Meta,
}

// --- Row (DB-facing) ---
#[derive(Debug, FromRow, Deserialize, HasId, Default)]
pub struct AccountRow {
    pub id: DbId,

    // Identity
    pub email: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,

    // Global Status
    pub enabled: bool,
    pub verified: bool,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: AccountMeta,

    #[sqlx(flatten)]
    pub audit: AuditFields,
}

// The struct to hold the combined result
#[derive(FromRow, Debug, Deserialize, HasId)]
pub struct AccountWithCredentials {
    pub id: DbId,
    #[sqlx(flatten)]
    pub account: AccountRow,
    #[sqlx(json)]
    pub credentials: Vec<JoinedCredentialOnAccount>,
}

#[derive(FromRow, Debug, Deserialize, HasId)]
pub struct JoinedCredentialOnAccount {
    pub id: DbId,

    pub account_id: DbId,
    pub workspace_id: DbId,
    pub kind: CredentialKind,
    pub provider: CredentialProvider,
    pub status: CredentialStatus,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    pub secret: Option<String>,
    pub last_used_at: Option<OffsetDateTime>,
    pub tags: Vec<String>,
    pub created_by: DbId,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub updated_by: Option<DbId>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

// --- Create (store input) ---
#[derive(Debug, Fields)]
pub struct AccountForCreate {
    pub email: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,

    pub enabled: bool,
    pub verified: bool,

    pub tags: Vec<String>,
    pub meta: AccountMeta,
}

// --- Update (store input) ---
#[derive(Debug, Fields, Clone)]
pub struct AccountForUpdate {
    pub email: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar_url: Option<String>,

    pub enabled: Option<bool>,
    pub verified: Option<bool>,

    pub tags: Option<Vec<String>>,
    pub meta: Option<AccountMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AccountMeta {
    pub schema_version: String,
}

impl Nullable for AccountMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<AccountMeta> for SeaValue {
    fn from(value: AccountMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for queries
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]

pub struct AccountFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    // NOTE: server limitation on derive(FilterNodes), if filter is used on any join queries need to define base table_name here on the rel attribute. if base table is not defined then could cause ambiguous WHERE query on JOIN statement.
    #[modql(rel = "account")]
    pub email: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub description: Option<OpValsString>,
    pub avatar_url: Option<OpValsString>,

    #[modql(to_sea_value_fn = "to_sea_bool")]
    pub verified: Option<OpValsValue>, // bool
    #[modql(to_sea_value_fn = "to_sea_bool")]
    pub enabled: Option<OpValsValue>, // bool

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

impl TryFrom<JsonValue> for AccountFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

#[cfg(test)]
impl Default for AccountForCreate {
    fn default() -> Self {
        use crate::store::utils::gen_rand_str;

        let email = format!("{}@{}.com", gen_rand_str(5), gen_rand_str(5));
        Self {
            email: email,
            name: gen_rand_str(10),
            description: Some("Fixture account for create() test".into()),
            avatar_url: Some("avatar_url.com".into()),
            verified: false,
            enabled: true,
            tags: vec![],
            meta: AccountMeta {
                schema_version: "1".into(),
            },
        }
    }
}

#[cfg(test)]
impl Default for AccountForUpdate {
    fn default() -> Self {
        Self {
            name: None,
            email: None,

            // Profile / status
            description: None,
            avatar_url: None,
            verified: None,
            enabled: None,

            // Free-form meta; keep structure present but empty
            meta: None,
            tags: None,
        }
    }
}

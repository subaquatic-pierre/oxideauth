use std::str::FromStr;

use derive_more::Display;
use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use oxideauth_macros::{EnumTextType, HasId};
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::impl_has_active_filter;
use crate::store::entities::audit::AuditFields;
use crate::store::entities::credential::{
    CredentialKind, CredentialProvider, CredentialRow, CredentialStatus,
};
use crate::store::entities::id::DbId;
use crate::store::error::{StoreError, StoreResult};
use crate::store::filter;
use crate::store::traits::meta::HasId;
use crate::store::utils::{json_to_sea_value, time_to_sea_value, to_sea_bool};

#[derive(Iden, Copy, Clone)]
pub enum AccountIden {
    #[iden = "account"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    AccountId,
    Membership,
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
    pub kind: AccountKind,

    // Global Status
    pub enabled: bool,
    pub version: i64,
    pub verified: bool,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: AccountMeta,

    #[sqlx(flatten)]
    pub audit: AuditFields,
}

#[derive(Debug, Serialize, Deserialize, Clone, EnumTextType)]
pub enum AccountKind {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "service")]
    Service,
}

impl Default for AccountKind {
    fn default() -> Self {
        AccountKind::User
    }
}

impl From<AccountKind> for SeaValue {
    fn from(value: AccountKind) -> Self {
        let s = format!("{value}");
        SeaValue::String(Some(s))
    }
}

impl Nullable for AccountKind {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
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

    pub kind: AccountKind,

    pub enabled: bool,
    pub verified: bool,

    pub tags: Vec<String>,
    pub meta: AccountMeta,
}

// --- Update (store input) ---
#[derive(Debug, Fields, Clone)]
pub struct AccountForUpdate {
    // pub email: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar_url: Option<String>,

    pub enabled: Option<bool>,
    pub version: Option<i64>,
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

#[cfg(any(test, feature = "integration"))]
impl Default for AccountForCreate {
    fn default() -> Self {
        use crate::store::utils::gen_rand_str;

        let email = format!("{}@{}.com", gen_rand_str(5), gen_rand_str(5));
        Self {
            email: email,
            name: gen_rand_str(10),
            description: Some("Fixture account for create() test".into()),
            avatar_url: Some("avatar_url.com".into()),
            kind: AccountKind::default(),
            verified: false,
            enabled: true,
            tags: vec![],
            meta: AccountMeta {
                schema_version: "1".into(),
            },
        }
    }
}

#[cfg(any(test, feature = "integration"))]
impl Default for AccountForUpdate {
    fn default() -> Self {
        Self {
            name: None,
            // email: None,

            // Profile / status
            description: None,
            avatar_url: None,
            verified: None,
            enabled: None,
            version: None,

            // Free-form meta; keep structure present but empty
            meta: None,
            tags: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::{Nullable, Value as SeaValue};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn test_account_kind_default() {
        // -- Execute
        let kind = AccountKind::default();

        // -- Assert
        assert!(matches!(kind, AccountKind::User));
    }

    #[test]
    fn test_account_kind_into_sea_value() {
        // -- Execute
        let user: SeaValue = AccountKind::User.into();
        let service: SeaValue = AccountKind::Service.into();

        // -- Assert
        assert_eq!(user, SeaValue::String(Some("user".to_string())));
        assert_eq!(service, SeaValue::String(Some("service".to_string())));
    }

    #[test]
    fn test_account_kind_nullable_null() {
        // -- Execute
        let null = <AccountKind as Nullable>::null();

        // -- Assert
        assert_eq!(null, SeaValue::Json(None));
    }

    #[test]
    fn test_account_filter_try_from_json() {
        // -- Setup
        let email_filter: AccountFilter = json!({"email": {"$contains": "x"}}).try_into().unwrap();
        let verified_filter: AccountFilter = json!({"verified": true}).try_into().unwrap();

        // -- Assert
        assert!(
            email_filter.email.is_some(),
            "email filter should parse into an OpValsString"
        );
        assert!(
            verified_filter.verified.is_some(),
            "verified filter should parse into an OpValsValue"
        );
    }

    #[test]
    fn test_account_filter_try_from_json_invalid() {
        // -- Execute
        let res: Result<AccountFilter, _> = json!({"email": 123}).try_into();

        // -- Assert
        assert!(res.is_err(), "non-string email should fail to deserialize");
    }

    #[test]
    fn test_account_row_default() {
        // -- Execute
        let row = AccountRow::default();

        // -- Assert
        assert_eq!(row.id.0, Uuid::nil());
        assert_eq!(row.email, "");
        assert_eq!(row.name, "");
        assert!(row.description.is_none());
        assert!(row.avatar_url.is_none());
        assert!(matches!(row.kind, AccountKind::User));
        assert!(!row.enabled);
        assert!(!row.verified);
        assert_eq!(row.version, 0);
        assert!(row.tags.is_empty());
        assert_eq!(row.meta.schema_version, "");
    }

    #[test]
    fn test_account_meta_default() {
        // -- Execute
        let meta = AccountMeta::default();

        // -- Assert
        assert_eq!(meta.schema_version, "");
    }
}

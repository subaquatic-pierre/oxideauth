use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use oxideauth_macros::HasId;
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::audit::AuditFields;
use crate::store::entities::id::DbId;
use crate::store::error::{StoreError, StoreResult};
use crate::store::traits::meta::HasId;
use crate::store::utils::{gen_rand_str, json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]
pub enum ProfileIden {
    #[iden = "profile"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    AccountId,
    WorkspaceId,
    #[iden = "email"]
    Email,
    Tags,
    Meta,
}

// --- Row (DB-facing) ---
/// Maps to the `profile` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId, Default)]
pub struct ProfileRow {
    pub id: DbId,

    pub account_id: Uuid,
    pub workspace_id: Uuid,

    // Workspace-facing contact email (decoupled from the account email)
    pub email: String,

    // Workspace-facing identity / presentation
    pub name: String,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub timezone: Option<String>,
    pub avatar_url: Option<String>,

    pub version: i64,
    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: ProfileMeta,

    #[sqlx(flatten)]
    pub audit: AuditFields,
}

// --- Create (store input) ---
#[derive(Debug, Fields)]
pub struct ProfileForCreate {
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub name: String,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub timezone: Option<String>,
    pub avatar_url: Option<String>,
    pub tags: Vec<String>,
    pub meta: ProfileMeta,
}

// --- Update (store input) ---
#[derive(Debug, Fields, Clone)]
pub struct ProfileForUpdate {
    pub name: Option<String>,
    pub email: Option<String>,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub timezone: Option<String>,
    pub avatar_url: Option<String>,
    pub version: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProfileMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ProfileMeta {
    pub schema_version: String,
}

impl Nullable for ProfileMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<ProfileMeta> for SeaValue {
    fn from(value: ProfileMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `profile` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct ProfileFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub account_id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub workspace_id: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub email: Option<OpValsString>,
    pub description: Option<OpValsString>,
    pub display_name: Option<OpValsString>,
    pub job_title: Option<OpValsString>,
    pub timezone: Option<OpValsString>,
    pub avatar_url: Option<OpValsString>,

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

impl TryFrom<JsonValue> for ProfileFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

#[cfg(test)]
impl Default for ProfileForCreate {
    fn default() -> Self {
        Self {
            account_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            email: format!("profile-{}@example.com", gen_rand_str(8)),
            name: format!("profile-{}", gen_rand_str(8)),
            description: Some("A default profile for testing.".into()),
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            tags: vec![],
            meta: ProfileMeta::default(),
        }
    }
}

#[cfg(test)]
impl Default for ProfileForUpdate {
    fn default() -> Self {
        Self {
            name: None,
            email: None,
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            version: None,
            tags: None,
            meta: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_profile_meta_default() {
        // -- Execute
        let meta = ProfileMeta::default();

        // -- Assert
        assert_eq!(meta.schema_version, "");
    }

    #[test]
    fn test_profile_row_default() {
        // -- Execute
        let row = ProfileRow::default();

        // -- Assert
        assert_eq!(row.id.0, Uuid::nil());
        assert_eq!(row.account_id, Uuid::nil());
        assert_eq!(row.workspace_id, Uuid::nil());
        assert_eq!(row.email, "");
        assert_eq!(row.name, "");
        assert!(row.description.is_none());
        assert!(row.display_name.is_none());
        assert!(row.job_title.is_none());
        assert!(row.timezone.is_none());
        assert!(row.avatar_url.is_none());
        assert_eq!(row.version, 0);
        assert!(row.tags.is_empty());
        assert_eq!(row.meta.schema_version, "");
    }

    #[test]
    fn test_profile_filter_try_from_json() {
        // -- Setup
        let filter: ProfileFilter = json!({"name": "alice"}).try_into().unwrap();
        let account_filter: ProfileFilter = json!({"account_id": "00000000-0000-0000-0000-000000000000"}).try_into().unwrap();

        // -- Assert
        assert!(filter.name.is_some(), "name filter should parse");
        assert!(
            account_filter.account_id.is_some(),
            "account_id filter should parse"
        );
    }
}

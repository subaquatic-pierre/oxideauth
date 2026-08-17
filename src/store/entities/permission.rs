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
use crate::store::utils::{json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]
pub enum PermissionIden {
    #[iden = "permission"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    Meta,
    Tags,
    Name,
    WorkspaceId,
}

#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct PermissionRow {
    pub id: DbId,
    pub workspace_id: Uuid,

    // Permission identity
    pub name: String,
    pub description: Option<String>,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: PermissionMeta,

    #[sqlx(flatten)]
    pub audit: AuditFields,
}

#[derive(Debug, Fields)]
pub struct PermissionForCreate {
    pub workspace_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PermissionMeta,
}

#[derive(Debug, Fields, Clone)]
pub struct PermissionForUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<PermissionMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PermissionMeta {
    pub schema_version: String,
}

impl Nullable for PermissionMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<PermissionMeta> for SeaValue {
    fn from(value: PermissionMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `permission` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct PermissionFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    #[modql(cast_as = "uuid", rel = "permission")]
    pub workspace_id: Option<OpValsString>,
    #[modql(rel = "permission")]
    pub name: Option<OpValsString>,
    pub description: Option<OpValsString>,

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

impl TryFrom<JsonValue> for PermissionFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

// --- Defaults for testing ---
#[cfg(any(test, feature = "integration"))]
impl Default for PermissionForCreate {
    fn default() -> Self {
        use crate::store::utils::gen_rand_str;

        Self {
            workspace_id: Some(Uuid::new_v4()),
            name: gen_rand_str(10),
            description: Some("A default permission for testing.".to_string()),
            tags: vec![],
            meta: PermissionMeta {
                schema_version: "1".to_string(),
            },
        }
    }
}

#[cfg(test)]
impl Default for PermissionForUpdate {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
            tags: None,
            meta: None,
        }
    }
}

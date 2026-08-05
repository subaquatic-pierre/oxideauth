use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::id::DbId;
use crate::store::entities::permission::PermissionMeta;
use crate::store::traits::meta::HasId;
use oxideauth_macros::HasId;

use crate::store::entities::audit::AuditFields;
use crate::store::error::{StoreError, StoreResult};
use crate::store::utils::{json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]
pub enum RoleIden {
    #[iden = "role"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    Tags,
    Meta,
    Permissions,
    RolePermission,
    RoleId,
    PermissionId,
    #[iden = "id"]
    PermissionPk,
    Permission,
}

// --- Row (DB-facing) ---
/// Maps to the `role` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct RoleRow {
    pub id: DbId,
    pub workspace_id: Uuid,

    // Role identity
    pub name: String,
    pub description: Option<String>,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: RoleMeta,

    #[sqlx(flatten)]
    pub audit: AuditFields,
}

// The struct to hold the combined result
#[derive(FromRow, Debug, Deserialize, HasId)]
pub struct RoleWithPermissions {
    pub id: DbId,
    #[sqlx(flatten)]
    pub role: RoleRow,
    #[sqlx(json)]
    pub permissions: Vec<JoinedPermissionOnRole>,
}

#[derive(FromRow, Debug, Deserialize, HasId)]
pub struct JoinedPermissionOnRole {
    pub id: DbId,
    pub workspace_id: Uuid,

    // Permission identity
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,

    pub tags: Vec<String>,
    pub meta: PermissionMeta,
    pub created_by: DbId,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub updated_by: Option<DbId>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

// --- Create (store input) ---
/// Input for creating a new `role`.
#[derive(Debug, Fields)]
pub struct RoleForCreate {
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: RoleMeta,
}

// --- Update (store input) ---
/// Input for updating an existing `role`.
#[derive(Debug, Fields, Clone)]
pub struct RoleForUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<RoleMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct RoleMeta {
    pub schema_version: String,
}

impl Nullable for RoleMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<RoleMeta> for SeaValue {
    fn from(value: RoleMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `role` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct RoleFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub workspace_id: Option<OpValsString>,
    #[modql(rel = "role")]
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

impl TryFrom<JsonValue> for RoleFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

// --- Defaults for testing ---
#[cfg(test)]
impl Default for RoleForCreate {
    fn default() -> Self {
        Self {
            workspace_id: Uuid::new_v4(),
            name: "default-role".to_string(),
            description: Some("A default role for testing.".to_string()),
            tags: vec![],
            meta: RoleMeta {
                schema_version: "1".to_string(),
            },
        }
    }
}

#[cfg(test)]
impl Default for RoleForUpdate {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
            tags: None,
            meta: None,
        }
    }
}

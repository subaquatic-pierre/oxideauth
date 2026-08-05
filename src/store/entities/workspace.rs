use oxideauth_macros::HasId;
use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use sea_query::{sea_value_to_json_value, Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::audit::{AuditFields, AuditMeta};
use crate::store::entities::id::DbId;
use crate::store::entities::project::{ProjectConfig, ProjectMeta};
use crate::store::error::{StoreError, StoreResult};
use crate::store::traits::meta::HasId;
use crate::store::utils::{json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]
pub enum WorkspaceIden {
    #[iden = "workspace"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    Tags,
    Meta,
    Project,
    Projects,
    WorkspaceId,
}

// --- Row (DB-facing) ---
/// Maps to the `workspace` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct WorkspaceRow {
    pub id: DbId,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,

    // Config
    #[sqlx(json)]
    pub config: WorkspaceConfig,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: WorkspaceMeta,

    #[sqlx(flatten)]
    pub audit: AuditFields,
}

// --- Row (DB-facing) ---
/// Maps to the `project` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct JoinedProjectOnWorkspace {
    pub id: DbId,
    pub workspace_id: Uuid,

    // Project identity
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,

    // Config
    #[sqlx(json)]
    pub config: ProjectConfig,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: ProjectMeta,

    pub created_by: DbId,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    pub updated_by: Option<DbId>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct WorkspaceWithProjects {
    pub id: DbId,
    #[sqlx(flatten)]
    pub workspace: WorkspaceRow,
    #[sqlx(json)]
    pub projects: Vec<JoinedProjectOnWorkspace>,
}

// --- Create (store input) ---
/// Input for creating a new `workspace`.
#[derive(Debug, Fields)]
pub struct WorkspaceForCreate {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub config: WorkspaceConfig,
    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,
}

// --- Update (store input) ---
/// Input for updating an existing `workspace`.
#[derive(Debug, Fields, Clone)]
pub struct WorkspaceForUpdate {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub config: Option<WorkspaceConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<WorkspaceMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct WorkspaceConfig {
    pub schema_version: String,
}

impl Nullable for WorkspaceConfig {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<WorkspaceConfig> for SeaValue {
    fn from(value: WorkspaceConfig) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct WorkspaceMeta {
    pub schema_version: String,
}

impl Nullable for WorkspaceMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<WorkspaceMeta> for SeaValue {
    fn from(value: WorkspaceMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `workspace` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct WorkspaceFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub slug: Option<OpValsString>,
    pub description: Option<OpValsString>,

    // NOTE: Filtering on JSONB fields like `config` and `meta` would require custom modql logic.
    // pub config: Option<OpValsValue>,
    // pub tags: Option<OpValsValue>,
    // pub meta: Option<OpValsValue>,

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

impl TryFrom<JsonValue> for WorkspaceFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

// TODO: uncomment cfg(test)

// --- Defaults for testing ---
// #[cfg(test)]
impl Default for WorkspaceForCreate {
    fn default() -> Self {
        use crate::store::utils::gen_rand_str;

        Self {
            name: gen_rand_str(10),
            slug: gen_rand_str(10),
            description: Some("A default workspace for testing.".into()),
            config: WorkspaceConfig {
                schema_version: "1".into(),
            },
            tags: vec![],
            meta: WorkspaceMeta {
                schema_version: "1".into(),
            },
        }
    }
}

// #[cfg(test)]
impl Default for WorkspaceForUpdate {
    fn default() -> Self {
        Self {
            name: None,
            slug: None,
            description: None,
            config: None,
            tags: None,
            meta: None,
        }
    }
}

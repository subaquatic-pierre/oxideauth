use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::id::DbId;
use crate::store::traits::meta::HasId;
use oxideauth_macros::HasId;

use crate::store::entities::audit::AuditFields;
use crate::store::error::{StoreError, StoreResult};
use crate::store::utils::{gen_rand_str, json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]
pub enum ProjectIden {
    #[iden = "project"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    Tags,
    Meta,
}

#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct ProjectRow {
    pub id: DbId,
    pub workspace_id: Uuid,

    // Project identity
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub owner: DbId,

    // Config
    #[sqlx(json)]
    pub config: ProjectConfig,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: ProjectMeta,

    #[sqlx(flatten)]
    pub audit: AuditFields,
}

#[derive(Debug, Fields)]
pub struct ProjectForCreate {
    pub workspace_id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub config: ProjectConfig,
    pub tags: Vec<String>,
    pub meta: ProjectMeta,
}

#[derive(Debug, Fields, Clone)]
pub struct ProjectForUpdate {
    pub name: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProjectMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ProjectConfig {
    pub schema_version: String,
}

impl Nullable for ProjectConfig {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<ProjectConfig> for SeaValue {
    fn from(value: ProjectConfig) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ProjectMeta {
    pub schema_version: String,
}

impl Nullable for ProjectMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<ProjectMeta> for SeaValue {
    fn from(value: ProjectMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `project` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct ProjectFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub workspace_id: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub code: Option<OpValsString>,
    pub description: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub owner: Option<OpValsString>,
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

impl TryFrom<JsonValue> for ProjectFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

#[cfg(test)]
impl Default for ProjectForCreate {
    fn default() -> Self {
        Self {
            workspace_id: Uuid::new_v4(),
            name: format!("project-{}", gen_rand_str(8)),
            code: Some(format!("project-{}", gen_rand_str(8))),
            description: Some("A default project for testing.".into()),
            config: ProjectConfig::default(),
            tags: vec![],
            meta: ProjectMeta::default(),
        }
    }
}

#[cfg(test)]
impl Default for ProjectForUpdate {
    fn default() -> Self {
        Self {
            name: None,
            code: None,
            description: None,
            config: None,
            tags: None,
            meta: None,
        }
    }
}

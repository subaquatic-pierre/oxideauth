use std::str::FromStr;

use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use oxideauth_macros::{EnumTextType, HasId};
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::id::DbId;
use crate::store::error::{StoreError, StoreResult};
use crate::store::traits::meta::HasId;
use crate::store::utils::{gen_rand_str, json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]
pub enum PolicyIden {
    #[iden = "policy"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    Meta,
    Tags,
    Name,
    WorkspaceId,
    Effect,
    PrincipalId,
    Actions,
    Resource,
    ConstraintExpr,
    Description,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, EnumTextType)]
#[serde(rename_all = "lowercase")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

impl From<PolicyEffect> for SeaValue {
    fn from(value: PolicyEffect) -> Self {
        let s = format!("{value}");
        SeaValue::String(Some(s))
    }
}

impl Nullable for PolicyEffect {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

// --- Row (DB-facing) ---
/// Maps to the `policy` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct PolicyRow {
    pub id: DbId,
    pub workspace_id: Uuid,

    // Policy identity
    pub name: Option<String>,

    // Policy body
    pub effect: PolicyEffect,
    pub principal_id: Option<Uuid>,
    pub actions: Vec<String>,
    pub resource: String,
    pub constraint_expr: Option<String>,
    pub description: Option<String>,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: PolicyMeta,

    // Audit (minimal: created_at / updated_at only)
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

// --- Create (store input) ---
/// Input for creating a new `policy`.
#[derive(Debug, Fields)]
pub struct PolicyForCreate {
    pub workspace_id: Option<Uuid>,
    pub name: Option<String>,
    pub effect: PolicyEffect,
    pub principal_id: Option<Uuid>,
    pub actions: Vec<String>,
    pub resource: String,
    pub constraint_expr: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PolicyMeta,
}

// --- Update (store input) ---
/// Input for updating an existing `policy`.
#[derive(Debug, Fields, Clone)]
pub struct PolicyForUpdate {
    pub name: Option<String>,
    pub effect: Option<PolicyEffect>,
    pub principal_id: Option<Uuid>,
    pub actions: Option<Vec<String>>,
    pub resource: Option<String>,
    pub constraint_expr: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<PolicyMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PolicyMeta {
    pub schema_version: String,
}

impl Nullable for PolicyMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<PolicyMeta> for SeaValue {
    fn from(value: PolicyMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `policy` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct PolicyFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub workspace_id: Option<OpValsString>,
    #[modql(rel = "policy")]
    pub name: Option<OpValsString>,
    pub effect: Option<OpValsString>,
    #[modql(cast_as = "uuid")]
    pub principal_id: Option<OpValsString>,
    pub resource: Option<OpValsString>,
    pub constraint_expr: Option<OpValsString>,
    pub description: Option<OpValsString>,

    // Audit filters (created_at / updated_at)
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub created_at: Option<OpValsValue>,
    #[modql(to_sea_value_fn = "time_to_sea_value")]
    pub updated_at: Option<OpValsValue>,
}

impl TryFrom<JsonValue> for PolicyFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

// --- Defaults for testing ---
#[cfg(any(test, feature = "integration"))]
impl Default for PolicyForCreate {
    fn default() -> Self {
        Self {
            workspace_id: Some(Uuid::new_v4()),
            name: Some(format!("policy-{}", gen_rand_str(8))),
            effect: PolicyEffect::Allow,
            principal_id: None,
            actions: vec!["membership:update".to_string()],
            resource: "self".to_string(),
            constraint_expr: None,
            description: Some("A default policy for testing.".to_string()),
            tags: vec![],
            meta: PolicyMeta {
                schema_version: "1".to_string(),
            },
        }
    }
}

#[cfg(test)]
impl Default for PolicyForUpdate {
    fn default() -> Self {
        Self {
            name: None,
            effect: None,
            principal_id: None,
            actions: None,
            resource: None,
            constraint_expr: None,
            description: None,
            tags: None,
            meta: None,
        }
    }
}

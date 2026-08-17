use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use oxideauth_macros::HasId;
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::store::entities::audit::AuditFields;
use crate::store::entities::id::DbId;
use crate::store::error::{StoreError, StoreResult};
use crate::store::traits::meta::HasId;
use crate::store::utils::{json_to_sea_value, time_to_sea_value};

#[derive(Iden, Copy, Clone)]
pub enum ClientIden {
    #[iden = "client"]
    Table, // TABLE_NAME
    Id, // TABLE_PK
    Tags,
    Meta,
}

// --- Row (DB-facing) ---
/// Maps to the `client` SQL table.
#[derive(Debug, FromRow, Deserialize, HasId)]
pub struct ClientRow {
    pub id: DbId,
    pub workspace_id: Uuid,

    // Client identity
    pub name: String,
    pub endpoint: Option<String>,
    pub description: Option<String>,

    pub tags: Vec<String>,
    #[sqlx(json)]
    pub meta: ClientMeta,

    #[sqlx(flatten)]
    pub audit: AuditFields,
}

// --- Create (store input) ---
/// Input for creating a new `client`.
#[derive(Debug, Fields)]
pub struct ClientForCreate {
    pub workspace_id: Option<Uuid>,
    pub name: String,
    pub endpoint: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: ClientMeta,
}

// --- Update (store input) ---
/// Input for updating an existing `client`.
#[derive(Debug, Default, Fields, Clone)]
pub struct ClientForUpdate {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ClientMeta>,
}

#[derive(Debug, Default, Fields, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ClientMeta {
    pub schema_version: String,
}

impl Nullable for ClientMeta {
    fn null() -> SeaValue {
        SeaValue::Json(None)
    }
}

impl From<ClientMeta> for SeaValue {
    fn from(value: ClientMeta) -> Self {
        json_to_sea_value(serde_json::to_value(value).unwrap()).unwrap()
    }
}

/// Filtering options for `client` queries.
#[derive(FilterNodes, Deserialize, Default, Debug, Clone)]
pub struct ClientFilter {
    #[modql(cast_as = "uuid")]
    pub id: Option<OpValsString>,
    #[modql(cast_as = "uuid", rel = "client")]
    pub workspace_id: Option<OpValsString>,
    #[modql(rel = "client")]
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

impl TryFrom<JsonValue> for ClientFilter {
    type Error = StoreError;

    fn try_from(value: JsonValue) -> StoreResult<Self> {
        let res = serde_json::from_value(value)?;
        Ok(res)
    }
}

// --- Defaults for testing ---
#[cfg(test)]
impl Default for ClientForCreate {
    fn default() -> Self {
        use crate::store::utils::gen_rand_str;

        Self {
            workspace_id: Some(Uuid::new_v4()),
            name: gen_rand_str(10),
            endpoint: Some("https://client.example.com".to_string()),
            description: Some("A default client for testing.".to_string()),
            tags: vec![],
            meta: ClientMeta {
                schema_version: "1".to_string(),
            },
        }
    }
}

use modql::field::Fields;
use modql::filter::{FilterNodes, OpValsString, OpValsValue};
use oxideauth_macros::HasId;
use sea_query::{Iden, Nullable, Value as SeaValue};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::prelude::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::store::entities::audit::{AuditFields, AuditMeta};
use crate::store::entities::credential::{CredentialProvider, DEFAULT_JWT_MAX_AGE};
use crate::store::entities::id::DbId;
use crate::store::entities::membership::MembershipStatus;
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
#[derive(Debug, Clone, FromRow, Deserialize, HasId, Default)]
pub struct WorkspaceRow {
    pub id: DbId,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub owner: DbId,

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
    pub owner: DbId,

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
    pub owner: DbId,
    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,
}

// --- Update (store input) ---
/// Input for updating an existing `workspace`.
#[derive(Debug, Fields, Clone)]
pub struct WorkspaceForUpdate {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub owner: Option<Uuid>,
    pub description: Option<String>,
    pub config: Option<WorkspaceConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<WorkspaceMeta>,
}

#[derive(Debug, Fields, Serialize, Deserialize, Clone)]
pub struct WorkspaceConfig {
    pub allowed_auth_providers: Vec<String>,
    pub jwt_max_age: i64,
    pub jwt_secret: String,
    pub public: bool,
    /// The status assigned to newly created memberships when the caller does
    /// not specify one (used by the membership email-resolve flow).
    #[serde(default)]
    pub default_membership_status: MembershipStatus,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            allowed_auth_providers: Default::default(),
            jwt_max_age: DEFAULT_JWT_MAX_AGE,
            // TODO: ensure jwt_secret set by service if blank
            // use workspace jwt_secret for token generation
            jwt_secret: "".to_string(),
            public: false,
            default_membership_status: MembershipStatus::default(),
        }
    }
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
    #[modql(cast_as = "uuid")]
    pub owner: Option<OpValsString>,

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
            owner: Uuid::nil().into(),
            config: WorkspaceConfig::default(),
            tags: vec![],
            meta: WorkspaceMeta::default(),
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
            owner: None,
            config: None,
            tags: None,
            meta: None,
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
    fn test_workspace_config_default() {
        // -- Execute
        let config = WorkspaceConfig::default();

        // -- Assert
        assert_eq!(config.jwt_max_age, DEFAULT_JWT_MAX_AGE);
        assert!(!config.public);
        assert!(config.allowed_auth_providers.is_empty());
        assert_eq!(config.jwt_secret, "");
        assert_eq!(
            config.default_membership_status,
            MembershipStatus::default()
        );
    }

    #[test]
    fn test_workspace_config_legacy_json_missing_default_status() {
        // Legacy JSONB config without `default_membership_status` must still
        // deserialize (the field is `#[serde(default)]`).
        let legacy: WorkspaceConfig = serde_json::from_value(json!({
            "allowed_auth_providers": ["google"],
            "jwt_max_age": 120,
            "jwt_secret": "s3cret",
            "public": true,
        }))
        .unwrap();

        assert_eq!(legacy.default_membership_status, MembershipStatus::Invited);
        assert_eq!(legacy.jwt_max_age, 120);
        assert!(legacy.public);
    }

    #[test]
    fn test_workspace_row_default() {
        // -- Execute
        let row = WorkspaceRow::default();

        // -- Assert
        assert_eq!(row.id.0, Uuid::nil());
        assert_eq!(row.name, "");
        assert_eq!(row.slug, "");
        assert!(row.description.is_none());
        assert_eq!(row.owner.0, Uuid::nil());
        assert_eq!(row.config.jwt_max_age, DEFAULT_JWT_MAX_AGE);
        assert!(row.tags.is_empty());
        assert_eq!(row.meta.schema_version, "");
    }

    #[test]
    fn test_workspace_filter_try_from_json() {
        // -- Setup
        let slug_filter: WorkspaceFilter = json!({"slug": "my-workspace"}).try_into().unwrap();
        let name_filter: WorkspaceFilter = json!({"name": {"$contains": "my"}}).try_into().unwrap();

        // -- Assert
        assert!(
            slug_filter.slug.is_some(),
            "slug filter should parse into an OpValsString"
        );
        assert!(
            name_filter.name.is_some(),
            "name filter should parse into an OpValsString"
        );
    }

    #[test]
    fn test_workspace_meta_sea_value_and_nullable() {
        // -- Setup
        let meta = WorkspaceMeta::default();

        // -- Execute
        let v: SeaValue = meta.clone().into();
        let null = <WorkspaceMeta as Nullable>::null();

        // -- Assert
        assert_eq!(null, SeaValue::Json(None));
        match v {
            SeaValue::Json(Some(boxed)) => {
                let parsed: WorkspaceMeta = serde_json::from_value(*boxed).unwrap();
                assert_eq!(parsed.schema_version, "");
            }
            other => panic!("expected SeaValue::Json(Some(_)), got {other:?}"),
        }
    }

    #[test]
    fn test_workspace_config_sea_value_and_nullable() {
        // -- Setup
        let config = WorkspaceConfig::default();

        // -- Execute
        let v: SeaValue = config.clone().into();
        let null = <WorkspaceConfig as Nullable>::null();

        // -- Assert
        assert_eq!(null, SeaValue::Json(None));
        match v {
            SeaValue::Json(Some(boxed)) => {
                let parsed: WorkspaceConfig = serde_json::from_value(*boxed).unwrap();
                assert_eq!(parsed.jwt_max_age, DEFAULT_JWT_MAX_AGE);
                assert!(!parsed.public);
            }
            other => panic!("expected SeaValue::Json(Some(_)), got {other:?}"),
        }
    }
}

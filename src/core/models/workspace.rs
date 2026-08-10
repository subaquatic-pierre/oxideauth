use modql::filter::{OpValString, OpValsString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            oath::AuthProvider,
        },
        traits::{
            filter::{OpValIsString, OpValWorkspaceId},
            list::RequestListParams,
        },
    },
    store::entities::workspace::{
        WorkspaceConfig as StoreWorkspaceConfig, WorkspaceFilter as StoreWorkspaceFilter,
        WorkspaceForCreate, WorkspaceMeta as StoreWorkspaceMeta, WorkspaceRow,
    },
};

// TODO: better way to define global workspace id
pub const GLOBAL_WS_ID: &'static str = "10000000-0000-0000-0000-000000000001";
pub const DEFAULT_WS_ID: &'static str = "10000000-0000-0000-0000-000000000002";

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Workspace {
    pub id: Uuid,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,

    // Config
    pub config: WorkspaceConfig,

    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,

    // Audit Fields (timestamps, creators, updaters)
    pub audit: CoreAuditFields,
}

impl Workspace {
    pub fn global_ws_id() -> Uuid {
        Uuid::try_parse(GLOBAL_WS_ID).unwrap()
    }

    pub fn default_ws_id() -> Uuid {
        Uuid::try_parse(DEFAULT_WS_ID).unwrap()
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    allowed_auth_providers: Vec<AuthProvider>,
    jwt_max_age: u64,
    jwt_secret: String,
}

impl From<StoreWorkspaceConfig> for WorkspaceConfig {
    fn from(value: StoreWorkspaceConfig) -> Self {
        WorkspaceConfig {
            allowed_auth_providers: vec![],
            jwt_max_age: 6000,
            jwt_secret: "secret".to_string(),
        }
    }
}

impl From<WorkspaceRow> for Workspace {
    fn from(row: WorkspaceRow) -> Self {
        // Assuming DbId can be directly converted to Uuid (common for SQLX/Postgres)
        let id: Uuid = row.id.into();

        let config = WorkspaceConfig::default();

        Self {
            id,
            name: row.name,
            slug: row.slug,
            description: row.description,
            config: config,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceCreateParams {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,

    // Assuming these require values at creation based on store struct
    pub config: WorkspaceConfig,
    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,
}

impl From<WorkspaceConfig> for StoreWorkspaceConfig {
    fn from(value: WorkspaceConfig) -> Self {
        // Self {
        //     schema_version: "".to_string(),
        // }
        let config = StoreWorkspaceConfig::default();
        config
    }
}

impl From<WorkspaceCreateParams> for WorkspaceForCreate {
    fn from(value: WorkspaceCreateParams) -> Self {
        Self {
            name: value.name,
            slug: value.slug,
            description: value.description,
            config: value.config.into(),
            tags: value.tags,
            meta: value.meta,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceListParams {
    pub filter: Option<RequestFilterParams<WorkspaceFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<WorkspaceFilter> for WorkspaceListParams {
    fn filter(&self) -> Option<RequestFilterParams<WorkspaceFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceDeleteParams {
    pub id: Option<Uuid>,
    pub slug: Option<String>,
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceUpdateParams {
    // Workspace Identifier (one must be provided)
    pub id: Option<Uuid>,
    pub slug: Option<String>,

    // Fields to Update (mirroring WorkspaceForUpdate)
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<WorkspaceConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<WorkspaceMeta>,
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceDescribeParams {
    pub id: Option<Uuid>,
    pub slug: Option<String>,
}

pub type WorkspaceMeta = StoreWorkspaceMeta;
pub type WorkspaceFilter = StoreWorkspaceFilter;

impl OpValWorkspaceId for WorkspaceFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        None
    }
}

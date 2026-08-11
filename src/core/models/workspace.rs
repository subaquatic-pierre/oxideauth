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
        WorkspaceForCreate, WorkspaceForUpdate, WorkspaceMeta as StoreWorkspaceMeta, WorkspaceRow,
    },
};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Workspace {
    pub id: Uuid,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub owner: Uuid,

    // Config
    pub config: WorkspaceConfig,

    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,

    // Audit Fields (timestamps, creators, updaters)
    pub audit: CoreAuditFields,
}

pub type WorkspaceConfig = StoreWorkspaceConfig;

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
            owner: row.owner.into(),
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
    pub owner: Uuid,
    pub config: WorkspaceConfig,
    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,
}

impl From<WorkspaceCreateParams> for WorkspaceForCreate {
    fn from(value: WorkspaceCreateParams) -> Self {
        Self {
            name: value.name,
            slug: value.slug,
            description: value.description,
            owner: value.owner.into(),
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

impl From<WorkspaceUpdateParams> for WorkspaceForUpdate {
    fn from(params: WorkspaceUpdateParams) -> Self {
        Self {
            name: params.name,
            slug: None, // slug in params is an identifier, not an update field
            owner: None,
            description: params.description,
            config: params.config,
            tags: params.tags,
            meta: params.meta,
        }
    }
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

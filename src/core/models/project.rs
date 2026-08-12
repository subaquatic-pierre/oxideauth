use modql::filter::{ListOptions, OpValString, OpValsString, op_val_string};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            workspace::Workspace,
        },
        traits::{
            filter::{OpValIsString, OpValWorkspaceId},
            list::RequestListParams,
            params::ValidateParams,
        },
    },
    store::{
        entities::project::{
            ProjectConfig as StoreProjectConfig, ProjectFilter as StoreProjectFilter,
            ProjectForCreate, ProjectForUpdate, ProjectMeta as StoreProjectMeta, ProjectRow,
        },
        utils::ListOptionsValidator,
    },
};

impl From<ProjectCreateParams> for ProjectForCreate {
    fn from(params: ProjectCreateParams) -> Self {
        Self {
            workspace_id: params.workspace_id,
            name: params.name,
            code: params.code,
            description: params.description,
            owner: params.owner.unwrap_or_default().into(),
            config: params.config,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

impl From<ProjectUpdateParams> for ProjectForUpdate {
    fn from(params: ProjectUpdateParams) -> Self {
        Self {
            name: params.name,
            code: params.new_code,
            description: params.description,
            owner: None,
            config: params.config,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Project {
    pub id: Uuid,
    pub workspace: Workspace,

    // Project identity
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,

    // Config
    pub config: ProjectConfig,

    pub tags: Vec<String>,
    pub meta: ProjectMeta,

    // Audit Fields
    pub audit: CoreAuditFields,
}

impl Project {
    /// Constructs a Project model by hydrating it with the full Workspace entity.
    /// This is used by the service layer after fetching both the row and the related entity.
    pub fn from_row_with_workspace(row: ProjectRow, workspace: Workspace) -> CoreResult<Self> {
        // Ensure the ID matches (useful for validation, though generally guaranteed by join/lookup)
        if row.workspace_id != workspace.id {
            return Err(CoreError::InvalidParams(
                "row.workspace_id does not match workspace.id".to_string(),
            ));
        }

        let new_project = Self {
            id: row.id.into(),
            workspace, // Use the provided full Workspace entity
            name: row.name,
            code: row.code,
            description: row.description,
            config: row.config,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        };

        Ok(new_project)
    }
}

#[derive(Debug, Deserialize)]
pub struct ProjectCreateParams {
    pub workspace_id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub owner: Option<Uuid>,

    pub config: ProjectConfig,
    pub tags: Vec<String>,
    pub meta: ProjectMeta,
}

#[derive(Debug, Deserialize)]
pub struct ProjectDescribeParams {
    pub id: Option<Uuid>,
    pub code: Option<String>,
    pub workspace_id: Uuid,
}

impl ValidateParams for ProjectDescribeParams {
    fn validate(self) -> CoreResult<Self> {
        // TODO: ensure params are correct
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
pub struct ProjectUpdateParams {
    // Identifier
    pub id: Option<Uuid>,
    pub code: Option<String>,

    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub new_code: Option<String>,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProjectMeta>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectDeleteParams {
    pub id: Option<Uuid>,
    pub workspace_id: Uuid,
    pub code: Option<String>,
}

pub struct ProjectListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<ProjectFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<ProjectFilter> for ProjectListParams {
    fn filter(&self) -> Option<RequestFilterParams<ProjectFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

pub type ProjectConfig = StoreProjectConfig;
pub type ProjectMeta = StoreProjectMeta;
pub type ProjectFilter = StoreProjectFilter;

impl OpValWorkspaceId for ProjectFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        self.workspace_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

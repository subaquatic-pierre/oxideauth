use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    project::{
        Project, ProjectConfig, ProjectCreateParams, ProjectDeleteParams, ProjectDescribeParams,
        ProjectFilter, ProjectListParams, ProjectMeta, ProjectUpdateParams,
    },
};
use crate::core::traits::params::IntoParams;

// --- ProjectDescribeReq ---
#[derive(Deserialize)]
pub struct ProjectDescribeReq {
    pub id: Option<Uuid>,
    // Use 'code' instead of 'slug'
    pub code: Option<String>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<ProjectDescribeParams> for ProjectDescribeReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProjectDescribeParams> {
        Ok(ProjectDescribeParams {
            id: self.id,
            // Map 'code'
            code: self.code,
            workspace_id,
        })
    }
}

// --- ProjectDescribeRes (and Update/Create Response) ---
#[derive(Serialize, Debug)]
pub struct ProjectDescribeRes {
    pub id: Uuid,
    pub name: String,
    // Use 'code' instead of 'slug'
    pub code: Option<String>,
    pub description: Option<String>,

    // Config
    pub config: ProjectConfig,

    pub tags: Vec<String>,
    pub meta: ProjectMeta,

    // Audit Fields
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    // Note: The full Workspace is available in the Core Model,
    // but typically not serialized fully in sub-entity responses like this
    // unless explicitly needed (we are omitting it here).
}

// --- From<Project> for ProjectDescribeRes ---
// Implement From to convert the Core Project entity to the Web Response DTO
impl From<Project> for ProjectDescribeRes {
    fn from(proj: Project) -> Self {
        Self {
            id: proj.id,
            name: proj.name,
            // Map 'code'
            code: proj.code,
            description: proj.description,
            config: proj.config,
            tags: proj.tags,
            meta: proj.meta,
            created_at: proj.audit.created_at,
            updated_at: proj.audit.updated_at,
        }
    }
}

// --- ProjectCreateReq ---
#[derive(Deserialize)]
pub struct ProjectCreateReq {
    pub name: String,
    // Use 'code' instead of 'slug'
    pub code: Option<String>,
    pub description: Option<String>,

    pub config: ProjectConfig,
    pub tags: Vec<String>,
    pub meta: ProjectMeta,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<ProjectCreateParams> for ProjectCreateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProjectCreateParams> {
        Ok(ProjectCreateParams {
            workspace_id,
            name: self.name,
            // Map 'code'
            code: self.code,
            description: self.description,
            config: self.config,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- ProjectUpdateReq ---
#[derive(Deserialize)]
pub struct ProjectUpdateReq {
    // Identifier
    pub id: Option<Uuid>,
    // Use current 'code' instead of 'slug' for identifying the project
    pub code: Option<String>,

    // Fields to Update (all fields here are Option<T> to represent 'patch')
    pub name: Option<String>,
    // The new code value is requested as 'new_code' in the Core Params
    pub new_code: Option<String>,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProjectMeta>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<ProjectUpdateParams> for ProjectUpdateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProjectUpdateParams> {
        Ok(ProjectUpdateParams {
            id: self.id,
            code: self.code,
            workspace_id,
            name: self.name,
            new_code: self.new_code, // Map to new_code
            description: self.description,
            config: self.config,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- ProjectDeleteReq ---
#[derive(Deserialize)]
pub struct ProjectDeleteReq {
    pub id: Option<Uuid>,
    // Use 'code' instead of 'slug'
    pub code: Option<String>,
}

// Implement IntoParams<ProjectDeleteReq> for ProjectDeleteParams
impl IntoParams<ProjectDeleteParams> for ProjectDeleteReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProjectDeleteParams> {
        Ok(ProjectDeleteParams {
            id: self.id,
            workspace_id,
            code: self.code,
        })
    }
}

// --- ProjectDeleteRes ---
// Returns the full deleted entity, matching the service layer contract
#[derive(Serialize)]
pub struct ProjectDeleteRes {
    pub id: Uuid,
    // Use 'code' instead of 'slug'
    pub code: Option<String>,
    pub name: String,
}

impl From<Project> for ProjectDeleteRes {
    fn from(proj: Project) -> Self {
        Self {
            id: proj.id,
            // Map 'code'
            code: proj.code,
            name: proj.name,
        }
    }
}

// --- ProjectListReq ---
#[derive(Deserialize, Debug)]
pub struct ProjectListReq {
    // The filter and options are unchanged in structure but are mapped to ProjectFilter
    pub filter: Option<RequestFilterParams<ProjectFilter>>,
    pub options: Option<RequestListOptions>,
}

impl IntoParams<ProjectListParams> for ProjectListReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProjectListParams> {
        Ok(ProjectListParams {
            filter: self.filter,
            options: self.options,
            workspace_id,
        })
    }
}

// --- ProjectListRes ---
#[derive(Serialize, Debug)]
pub struct ProjectListRes {
    // Corrected field name from 'workspaces' to 'projects'
    pub projects: Vec<ProjectDescribeRes>,
    pub metadata: ListResponseMeta,
}

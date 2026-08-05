use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    project::{
        Project, ProjectConfig, ProjectCreateParams, ProjectDeleteParams, ProjectDescribeParams,
        ProjectFilter, ProjectListParams, ProjectMeta, ProjectUpdateParams,
    },
};

// --- ProjectDescribeReq ---
#[derive(Deserialize)]
pub struct ProjectDescribeReq {
    pub id: Option<Uuid>,
    // Use 'code' instead of 'slug'
    pub code: Option<String>,
    // Add the required workspace identifier for context
    pub workspace_id: Uuid,
}

// Implement From to convert Web Req to Core Param
impl From<ProjectDescribeReq> for ProjectDescribeParams {
    fn from(value: ProjectDescribeReq) -> Self {
        Self {
            id: value.id,
            // Map 'code'
            code: value.code,
            workspace_id: value.workspace_id,
        }
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
    pub workspace_id: Uuid,
    pub name: String,
    // Use 'code' instead of 'slug'
    pub code: Option<String>,
    pub description: Option<String>,

    pub config: ProjectConfig,
    pub tags: Vec<String>,
    pub meta: ProjectMeta,
}

// Implement From to convert Web Req to Core Param
impl From<ProjectCreateReq> for ProjectCreateParams {
    fn from(value: ProjectCreateReq) -> Self {
        Self {
            workspace_id: value.workspace_id,
            name: value.name,
            // Map 'code'
            code: value.code,
            description: value.description,
            config: value.config,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- ProjectUpdateReq ---
#[derive(Deserialize)]
pub struct ProjectUpdateReq {
    // Identifier
    pub id: Option<Uuid>,
    // Use current 'code' instead of 'slug' for identifying the project
    pub code: Option<String>,
    pub workspace_id: Uuid,

    // Fields to Update (all fields here are Option<T> to represent 'patch')
    pub name: Option<String>,
    // The new code value is requested as 'new_code' in the Core Params
    pub new_code: Option<String>,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProjectMeta>,
}

// Implement From to convert Web Req to Core Param
impl From<ProjectUpdateReq> for ProjectUpdateParams {
    fn from(value: ProjectUpdateReq) -> Self {
        Self {
            id: value.id,
            code: value.code,
            workspace_id: value.workspace_id,
            name: value.name,
            new_code: value.new_code, // Map to new_code
            description: value.description,
            config: value.config,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- ProjectDeleteReq ---
#[derive(Deserialize)]
pub struct ProjectDeleteReq {
    pub id: Option<Uuid>,
    pub workspace_id: Uuid,
    // Use 'code' instead of 'slug'
    pub code: Option<String>,
}

// Implement From<ProjectDeleteReq> for ProjectDeleteParams
impl From<ProjectDeleteReq> for ProjectDeleteParams {
    fn from(value: ProjectDeleteReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
            code: value.code,
        }
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
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<ProjectFilter>>,
    pub options: Option<RequestListOptions>,
}

impl From<ProjectListReq> for ProjectListParams {
    fn from(value: ProjectListReq) -> Self {
        Self {
            filter: value.filter,
            options: value.options,
            workspace_id: value.workspace_id,
        }
    }
}

// --- ProjectListRes ---
#[derive(Serialize, Debug)]
pub struct ProjectListRes {
    // Corrected field name from 'workspaces' to 'projects'
    pub projects: Vec<ProjectDescribeRes>,
    pub metadata: ListResponseMeta,
}

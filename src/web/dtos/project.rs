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
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<ProjectDescribeParams> for ProjectDescribeReq {
    fn into_params(self) -> CoreResult<ProjectDescribeParams> {
        Ok(ProjectDescribeParams {
            id: self.id,
            // Map 'code'
            code: self.code,
            workspace_id: self.workspace_id,
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
    pub owner: Option<Uuid>,
    pub code: Option<String>,
    pub description: Option<String>,

    pub config: ProjectConfig,
    pub tags: Vec<String>,
    pub meta: ProjectMeta,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<ProjectCreateParams> for ProjectCreateReq {
    fn into_params(self) -> CoreResult<ProjectCreateParams> {
        Ok(ProjectCreateParams {
            workspace_id: self.workspace_id,
            name: self.name,
            owner: self.owner,
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
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<ProjectUpdateParams> for ProjectUpdateReq {
    fn into_params(self) -> CoreResult<ProjectUpdateParams> {
        Ok(ProjectUpdateParams {
            id: self.id,
            code: self.code,
            workspace_id: self.workspace_id,
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
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

// Implement IntoParams<ProjectDeleteReq> for ProjectDeleteParams
impl IntoParams<ProjectDeleteParams> for ProjectDeleteReq {
    fn into_params(self) -> CoreResult<ProjectDeleteParams> {
        Ok(ProjectDeleteParams {
            id: self.id,
            workspace_id: self.workspace_id,
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
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<ProjectListParams> for ProjectListReq {
    fn into_params(self) -> CoreResult<ProjectListParams> {
        Ok(ProjectListParams {
            filter: self.filter,
            options: self.options,
            workspace_id: self.workspace_id,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_describe_req_into_params_by_id() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = ProjectDescribeReq {
            id: Some(id),
            code: None,
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.id, Some(id));
        assert_eq!(params.code, None);
        assert_eq!(params.workspace_id, Some(ws_id));
    }

    #[test]
    fn test_project_describe_req_into_params_by_code() {
        let ws_id = Uuid::new_v4();
        let params = ProjectDescribeReq {
            id: None,
            code: Some("p-1".to_string()),
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.id, None);
        assert_eq!(params.code.as_deref(), Some("p-1"));
        assert_eq!(params.workspace_id, Some(ws_id));
    }

    #[test]
    fn test_project_create_req_into_params() {
        let ws_id = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let params = ProjectCreateReq {
            name: "payments".to_string(),
            owner: Some(owner),
            code: Some("pay".to_string()),
            description: Some("Payments service".to_string()),
            config: ProjectConfig::default(),
            tags: vec!["svc".to_string()],
            meta: ProjectMeta::default(),
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_eq!(params.workspace_id, Some(ws_id));
        assert_eq!(params.name, "payments");
        assert_eq!(params.owner, Some(owner));
        assert_eq!(params.code.as_deref(), Some("pay"));
        assert_eq!(params.description.as_deref(), Some("Payments service"));
        assert_eq!(params.tags, vec!["svc".to_string()]);
        assert_eq!(params.meta.schema_version, ProjectMeta::default().schema_version);
    }

    #[test]
    fn test_project_update_req_into_params_maps_new_code() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = ProjectUpdateReq {
            id: Some(id),
            code: Some("pay".to_string()),
            name: Some("renamed".to_string()),
            new_code: Some("pay-v2".to_string()),
            description: None,
            config: None,
            tags: None,
            meta: None,
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_eq!(params.id, Some(id));
        assert_eq!(params.code.as_deref(), Some("pay"));
        assert_eq!(params.workspace_id, Some(ws_id));
        assert_eq!(params.name.as_deref(), Some("renamed"));
        assert_eq!(params.new_code.as_deref(), Some("pay-v2"));
        assert!(params.config.is_none());
        assert!(params.meta.is_none());
    }

    #[test]
    fn test_project_delete_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = ProjectDeleteReq {
            id: None,
            code: Some("pay".to_string()),
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert!(params.id.is_none());
        assert_eq!(params.code.as_deref(), Some("pay"));
        assert_eq!(params.workspace_id, Some(ws_id));
    }

    #[test]
    fn test_project_list_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = ProjectListReq {
            filter: None,
            options: None,
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.workspace_id, Some(ws_id));
        assert!(params.filter.is_none());
        assert!(params.options.is_none());
    }

    #[test]
    fn test_project_describe_res_from_project_default() {
        let proj = Project::default();
        let res = ProjectDescribeRes::from(proj);
        assert_eq!(res.id, Uuid::default());
        assert_eq!(res.name, String::default());
        assert!(res.code.is_none());
        assert_eq!(res.meta.schema_version, ProjectMeta::default().schema_version);
    }
}

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    workspace::{
        Workspace, WorkspaceConfig, WorkspaceCreateParams, WorkspaceDeleteParams,
        WorkspaceDescribeParams, WorkspaceFilter, WorkspaceListParams, WorkspaceMeta,
        WorkspaceUpdateParams,
    },
};

// --- WorkspaceDescribeReq ---
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkspaceDescribeReq {
    pub id: Option<Uuid>,
    pub slug: Option<String>,
}

// Implement From to convert Web Req to Core Param
impl From<WorkspaceDescribeReq> for WorkspaceDescribeParams {
    fn from(value: WorkspaceDescribeReq) -> Self {
        Self {
            id: value.id,
            slug: value.slug,
        }
    }
}

// --- WorkspaceDescribeRes (and Update/Create Response) ---
#[derive(Serialize, Debug)]
pub struct WorkspaceDescribeRes {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,

    // Config
    pub config: WorkspaceConfig,
    pub owner: Uuid,

    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,

    // Audit Fields
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

// --- From<Workspace> for WorkspaceDescribeRes ---
// Implement From to convert the Core Workspace entity to the Web Response DTO
impl From<Workspace> for WorkspaceDescribeRes {
    fn from(ws: Workspace) -> Self {
        Self {
            id: ws.id,
            name: ws.name,
            slug: ws.slug,
            description: ws.description,
            config: ws.config,
            owner: ws.owner,
            tags: ws.tags,
            meta: ws.meta,
            created_at: ws.audit.created_at,
            updated_at: ws.audit.updated_at,
        }
    }
}

// --- WorkspaceCreateReq ---
#[derive(Deserialize)]
pub struct WorkspaceCreateReq {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub owner: Option<Uuid>,
    pub config: Option<WorkspaceConfig>,
    pub tags: Vec<String>,
    pub meta: Option<WorkspaceMeta>,
}

// Implement From to convert Web Req to Core Param
impl From<WorkspaceCreateReq> for WorkspaceCreateParams {
    fn from(value: WorkspaceCreateReq) -> Self {
        Self {
            name: value.name,
            slug: value.slug,
            description: value.description,
            owner: value.owner,
            config: value.config.unwrap_or_default(),
            tags: value.tags,
            meta: value.meta.unwrap_or_default(),
        }
    }
}

// --- WorkspaceUpdateReq ---
#[derive(Deserialize)]
pub struct WorkspaceUpdateReq {
    // Identifier (one or both must be provided)
    pub id: Option<Uuid>,
    pub slug: Option<String>,

    // Fields to Update (all fields here are Option<T> to represent 'patch')
    pub owner: Option<Uuid>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<WorkspaceConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<WorkspaceMeta>,
}

// Implement From to convert Web Req to Core Param
impl From<WorkspaceUpdateReq> for WorkspaceUpdateParams {
    fn from(value: WorkspaceUpdateReq) -> Self {
        Self {
            id: value.id,
            slug: value.slug,
            name: value.name,
            description: value.description,
            owner: value.owner,
            config: value.config,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- WorkspaceDeleteReq ---
#[derive(Deserialize)]
pub struct WorkspaceDeleteReq {
    pub id: Option<Uuid>,
    pub slug: Option<String>,
}

// Implement From<WorkspaceDeleteReq> for WorkspaceDeleteParams
impl From<WorkspaceDeleteReq> for WorkspaceDeleteParams {
    fn from(value: WorkspaceDeleteReq) -> Self {
        Self {
            id: value.id,
            slug: value.slug,
        }
    }
}

// --- WorkspaceDeleteRes ---
// Returns the full deleted entity, matching the service layer contract
#[derive(Serialize)]
pub struct WorkspaceDeleteRes {
    // Reusing fields from WorkspaceDescribeRes for simplicity, or define a new struct:
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

impl From<Workspace> for WorkspaceDeleteRes {
    fn from(ws: Workspace) -> Self {
        Self {
            id: ws.id,
            slug: ws.slug,
            name: ws.name,
        }
    }
}

// --- WorkspaceListReq ---
#[derive(Deserialize, Debug)]
pub struct WorkspaceListReq {
    pub filter: Option<RequestFilterParams<WorkspaceFilter>>,
    pub options: Option<RequestListOptions>,
}

impl From<WorkspaceListReq> for WorkspaceListParams {
    fn from(value: WorkspaceListReq) -> Self {
        Self {
            filter: value.filter,
            options: value.options,
        }
    }
}

// --- WorkspaceListRes ---
// This is the payload that will be wrapped by the WebResponse envelope
#[derive(Serialize, Debug)]
pub struct WorkspaceListRes {
    pub workspaces: Vec<WorkspaceDescribeRes>, // Use the response DTO for the list
    pub metadata: ListResponseMeta,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::CoreError;

    #[test]
    fn test_workspace_describe_req_into_params_by_id() {
        let id = Uuid::new_v4();
        let params = WorkspaceDescribeParams::from(WorkspaceDescribeReq {
            id: Some(id),
            slug: None,
        });
        assert_eq!(params.id, Some(id));
        assert!(params.slug.is_none());
    }

    #[test]
    fn test_workspace_describe_req_into_params_by_slug() {
        let params = WorkspaceDescribeParams::from(WorkspaceDescribeReq {
            id: None,
            slug: Some("acme".to_string()),
        });
        assert!(params.id.is_none());
        assert_eq!(params.slug.as_deref(), Some("acme"));
    }

    #[test]
    fn test_workspace_describe_params_id_or_slug_valid() {
        let id = Uuid::new_v4();
        let params = WorkspaceDescribeParams {
            id: Some(id),
            slug: None,
        };
        assert_eq!(params.id_or_slug().unwrap(), id.to_string());

        let params = WorkspaceDescribeParams {
            id: None,
            slug: Some("acme".to_string()),
        };
        assert_eq!(params.id_or_slug().unwrap(), "acme");
    }

    #[test]
    fn test_workspace_describe_params_id_or_slug_missing() {
        let params = WorkspaceDescribeParams {
            id: None,
            slug: None,
        };
        let err = params.id_or_slug().unwrap_err();
        assert!(matches!(err, CoreError::InvalidParams(msg) if msg == "ID or slug required"));
    }

    #[test]
    fn test_workspace_create_req_into_params_defaults() {
        let params = WorkspaceCreateParams::from(WorkspaceCreateReq {
            name: "Acme".to_string(),
            slug: "acme".to_string(),
            description: None,
            owner: None,
            config: None,
            tags: vec![],
            meta: None,
        });
        assert_eq!(params.name, "Acme");
        assert_eq!(params.slug, "acme");
        assert!(params.description.is_none());
        assert!(params.owner.is_none());
        assert!(params.config.allowed_auth_providers.is_empty());
        assert_eq!(params.meta.schema_version, WorkspaceMeta::default().schema_version);
    }

    #[test]
    fn test_workspace_create_req_into_params_with_values() {
        let owner = Uuid::new_v4();
        let params = WorkspaceCreateParams::from(WorkspaceCreateReq {
            name: "Acme".to_string(),
            slug: "acme".to_string(),
            description: Some("desc".to_string()),
            owner: Some(owner),
            config: Some(WorkspaceConfig::default()),
            tags: vec!["t".to_string()],
            meta: Some(WorkspaceMeta::default()),
        });
        assert_eq!(params.description.as_deref(), Some("desc"));
        assert_eq!(params.owner, Some(owner));
        assert_eq!(params.tags, vec!["t".to_string()]);
        assert_eq!(params.meta.schema_version, WorkspaceMeta::default().schema_version);
    }

    #[test]
    fn test_workspace_update_req_into_params() {
        let id = Uuid::new_v4();
        let params = WorkspaceUpdateParams::from(WorkspaceUpdateReq {
            id: Some(id),
            slug: None,
            owner: Some(Uuid::new_v4()),
            name: Some("Renamed".to_string()),
            description: None,
            config: None,
            tags: None,
            meta: None,
        });
        assert_eq!(params.id, Some(id));
        assert!(params.slug.is_none());
        assert_eq!(params.name.as_deref(), Some("Renamed"));
        assert!(params.owner.is_some());
        assert!(params.meta.is_none());
    }

    #[test]
    fn test_workspace_delete_req_into_params() {
        let params = WorkspaceDeleteParams::from(WorkspaceDeleteReq {
            id: None,
            slug: Some("acme".to_string()),
        });
        assert!(params.id.is_none());
        assert_eq!(params.slug.as_deref(), Some("acme"));
    }

    #[test]
    fn test_workspace_list_req_into_params() {
        let params = WorkspaceListParams::from(WorkspaceListReq {
            filter: None,
            options: None,
        });
        assert!(params.filter.is_none());
        assert!(params.options.is_none());
    }

    #[test]
    fn test_workspace_describe_res_from_workspace_default() {
        let ws = Workspace::default();
        let res = WorkspaceDescribeRes::from(ws);
        assert_eq!(res.id, Uuid::default());
        assert_eq!(res.name, String::default());
        assert_eq!(res.slug, String::default());
        assert_eq!(res.owner, Uuid::default());
        assert!(res.tags.is_empty());
    }
}

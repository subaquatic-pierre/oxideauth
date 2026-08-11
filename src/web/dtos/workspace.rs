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
#[derive(Deserialize)]
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
    pub owner: Uuid,
    // Must be provided, but can be the default value
    pub config: WorkspaceConfig,
    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,
}

// Implement From to convert Web Req to Core Param
impl From<WorkspaceCreateReq> for WorkspaceCreateParams {
    fn from(value: WorkspaceCreateReq) -> Self {
        Self {
            name: value.name,
            slug: value.slug,
            description: value.description,
            owner: value.owner,
            config: value.config,
            tags: value.tags,
            meta: value.meta,
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

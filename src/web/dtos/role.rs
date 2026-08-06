use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    permission::Permission,
    role::{
        Role, RoleCreateParams, RoleDeleteParams, RoleDescribeParams, RoleFilter, RoleListParams,
        RoleMeta, RoleUpdateParams,
    },
};

// --- RoleDescribeReq ---
#[derive(Deserialize)]
pub struct RoleDescribeReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

impl From<RoleDescribeReq> for RoleDescribeParams {
    fn from(value: RoleDescribeReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
        }
    }
}

// --- RoleDescribeRes ---
#[derive(Serialize, Debug)]
pub struct RoleDescribeRes {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,
    pub tags: Vec<String>,
    pub meta: RoleMeta,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

impl From<Role> for RoleDescribeRes {
    fn from(role: Role) -> Self {
        Self {
            id: role.id,
            name: role.name,
            description: role.description,
            permissions: role.permissions,
            tags: role.tags,
            meta: role.meta,
            created_at: role.audit.created_at,
            updated_at: role.audit.updated_at,
        }
    }
}

// --- RoleCreateReq ---
#[derive(Deserialize)]
pub struct RoleCreateReq {
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub permission_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub meta: RoleMeta,
}

impl From<RoleCreateReq> for RoleCreateParams {
    fn from(value: RoleCreateReq) -> Self {
        Self {
            workspace_id: value.workspace_id,
            name: value.name,
            description: value.description,
            permission_ids: value.permission_ids,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- RoleUpdateReq ---
#[derive(Deserialize)]
pub struct RoleUpdateReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub permission_ids: Option<Vec<Uuid>>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<RoleMeta>,
}

impl From<RoleUpdateReq> for RoleUpdateParams {
    fn from(value: RoleUpdateReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
            name: value.name,
            description: value.description,
            permission_ids: value.permission_ids,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- RoleListReq ---
#[derive(Deserialize, Debug)]
pub struct RoleListReq {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<RoleFilter>>,
    pub options: Option<RequestListOptions>,
}

impl From<RoleListReq> for RoleListParams {
    fn from(value: RoleListReq) -> Self {
        Self {
            workspace_id: value.workspace_id,
            filter: value.filter,
            options: value.options,
        }
    }
}

// --- RoleListRes ---
#[derive(Serialize, Debug)]
pub struct RoleListRes {
    pub roles: Vec<RoleDescribeRes>,
    pub metadata: ListResponseMeta,
}

// --- RoleDeleteReq ---
#[derive(Deserialize)]
pub struct RoleDeleteReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

impl From<RoleDeleteReq> for RoleDeleteParams {
    fn from(value: RoleDeleteReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
        }
    }
}

// --- RoleDeleteRes ---
#[derive(Serialize)]
pub struct RoleDeleteRes {
    pub id: Uuid,
}

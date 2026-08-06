use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    permission::{
        Permission, PermissionCreateParams, PermissionDeleteParams, PermissionDescribeParams,
        PermissionFilter, PermissionListParams, PermissionMeta, PermissionUpdateParams,
    },
};

// --- PermissionDescribeReq ---
#[derive(Deserialize)]
pub struct PermissionDescribeReq {
    pub id: Option<Uuid>,
    pub workspace_id: Uuid,
    pub code: Option<String>,
}

impl From<PermissionDescribeReq> for PermissionDescribeParams {
    fn from(value: PermissionDescribeReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
            code: value.code,
        }
    }
}

// --- PermissionDescribeRes ---
#[derive(Serialize, Debug)]
pub struct PermissionDescribeRes {
    pub id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PermissionMeta,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

impl From<Permission> for PermissionDescribeRes {
    fn from(perm: Permission) -> Self {
        Self {
            id: perm.id,
            name: perm.name,
            code: perm.code,
            description: perm.description,
            tags: perm.tags,
            meta: perm.meta,
            created_at: perm.audit.created_at,
            updated_at: perm.audit.updated_at,
        }
    }
}

// --- PermissionCreateReq ---
#[derive(Deserialize)]
pub struct PermissionCreateReq {
    pub workspace_id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PermissionMeta,
}

impl From<PermissionCreateReq> for PermissionCreateParams {
    fn from(value: PermissionCreateReq) -> Self {
        Self {
            workspace_id: value.workspace_id,
            name: value.name,
            code: value.code,
            description: value.description,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- PermissionUpdateReq ---
#[derive(Deserialize)]
pub struct PermissionUpdateReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<PermissionMeta>,
}

impl From<PermissionUpdateReq> for PermissionUpdateParams {
    fn from(value: PermissionUpdateReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
            name: value.name,
            code: value.code,
            description: value.description,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- PermissionListReq ---
#[derive(Deserialize, Debug)]
pub struct PermissionListReq {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<PermissionFilter>>,
    pub options: Option<RequestListOptions>,
}

impl From<PermissionListReq> for PermissionListParams {
    fn from(value: PermissionListReq) -> Self {
        Self {
            workspace_id: value.workspace_id,
            filter: value.filter,
            options: value.options,
        }
    }
}

// --- PermissionListRes ---
#[derive(Serialize, Debug)]
pub struct PermissionListRes {
    pub permissions: Vec<PermissionDescribeRes>,
    pub metadata: ListResponseMeta,
}

// --- PermissionDeleteReq ---
#[derive(Deserialize)]
pub struct PermissionDeleteReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

impl From<PermissionDeleteReq> for PermissionDeleteParams {
    fn from(value: PermissionDeleteReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
        }
    }
}

// --- PermissionDeleteRes ---
#[derive(Serialize)]
pub struct PermissionDeleteRes {
    pub id: Uuid,
}

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    permission::{
        Permission, PermissionCreateParams, PermissionDeleteParams, PermissionDescribeParams,
        PermissionFilter, PermissionListParams, PermissionMeta, PermissionUpdateParams,
    },
};
use crate::core::traits::params::IntoParams;

// --- PermissionDescribeReq ---
#[derive(Deserialize)]
pub struct PermissionDescribeReq {
    pub id: Option<Uuid>,
}

impl IntoParams<PermissionDescribeParams> for PermissionDescribeReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PermissionDescribeParams> {
        Ok(PermissionDescribeParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- PermissionDescribeRes ---
#[derive(Serialize, Debug)]
pub struct PermissionDescribeRes {
    pub id: Uuid,
    pub name: String,
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
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PermissionMeta,
}

impl IntoParams<PermissionCreateParams> for PermissionCreateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PermissionCreateParams> {
        Ok(PermissionCreateParams {
            workspace_id,
            name: self.name,
            description: self.description,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- PermissionUpdateReq ---
#[derive(Deserialize)]
pub struct PermissionUpdateReq {
    pub id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<PermissionMeta>,
}

impl IntoParams<PermissionUpdateParams> for PermissionUpdateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PermissionUpdateParams> {
        Ok(PermissionUpdateParams {
            id: self.id,
            workspace_id,
            name: self.name,
            description: self.description,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- PermissionListReq ---
#[derive(Deserialize, Debug)]
pub struct PermissionListReq {
    pub filter: Option<RequestFilterParams<PermissionFilter>>,
    pub options: Option<RequestListOptions>,
}

impl IntoParams<PermissionListParams> for PermissionListReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PermissionListParams> {
        Ok(PermissionListParams {
            workspace_id,
            filter: self.filter,
            options: self.options,
        })
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
}

impl IntoParams<PermissionDeleteParams> for PermissionDeleteReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PermissionDeleteParams> {
        Ok(PermissionDeleteParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- PermissionDeleteRes ---
#[derive(Serialize)]
pub struct PermissionDeleteRes {
    pub id: Uuid,
}

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    permission::Permission,
    role::{
        Role, RoleCreateParams, RoleDeleteParams, RoleDescribeParams, RoleFilter, RoleListParams,
        RoleMeta, RoleUpdateParams,
    },
};
use crate::core::traits::params::IntoParams;

// --- RoleDescribeReq ---
#[derive(Deserialize)]
pub struct RoleDescribeReq {
    pub id: Uuid,
}

impl IntoParams<RoleDescribeParams> for RoleDescribeReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<RoleDescribeParams> {
        Ok(RoleDescribeParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- RoleDescribeRes ---
#[derive(Serialize, Debug)]
pub struct RoleDescribeRes {
    pub id: Uuid,
    pub workspace_id: Uuid,
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
            workspace_id: role.workspace_id,
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
    pub name: String,
    pub description: Option<String>,
    pub permission_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub meta: RoleMeta,
}

impl IntoParams<RoleCreateParams> for RoleCreateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<RoleCreateParams> {
        Ok(RoleCreateParams {
            workspace_id,
            name: self.name,
            description: self.description,
            permission_ids: self.permission_ids,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- RoleUpdateReq ---
#[derive(Deserialize)]
pub struct RoleUpdateReq {
    pub id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub permission_ids: Option<Vec<Uuid>>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<RoleMeta>,
}

impl IntoParams<RoleUpdateParams> for RoleUpdateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<RoleUpdateParams> {
        Ok(RoleUpdateParams {
            id: self.id,
            workspace_id,
            name: self.name,
            description: self.description,
            permission_ids: self.permission_ids,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- RoleListReq ---
#[derive(Deserialize, Debug)]
pub struct RoleListReq {
    pub filter: Option<RequestFilterParams<RoleFilter>>,
    pub options: Option<RequestListOptions>,
}

impl IntoParams<RoleListParams> for RoleListReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<RoleListParams> {
        Ok(RoleListParams {
            workspace_id,
            filter: self.filter,
            options: self.options,
        })
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
}

impl IntoParams<RoleDeleteParams> for RoleDeleteReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<RoleDeleteParams> {
        Ok(RoleDeleteParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- RoleDeleteRes ---
#[derive(Serialize)]
pub struct RoleDeleteRes {
    pub id: Uuid,
}

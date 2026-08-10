use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    membership::{
        Membership, MembershipCreateParams, MembershipDeleteParams, MembershipDescribeParams,
        MembershipFilter, MembershipListParams, MembershipMeta, MembershipUpdateParams,
    },
    role::Role,
};
use crate::core::traits::params::IntoParams;
use crate::store::entities::membership::{MembershipScope, MembershipStatus};

// --- MembershipDescribeReq ---
#[derive(Deserialize)]
pub struct MembershipDescribeReq {
    pub id: Uuid,
}

impl IntoParams<MembershipDescribeParams> for MembershipDescribeReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<MembershipDescribeParams> {
        Ok(MembershipDescribeParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- MembershipDescribeRes ---
#[derive(Serialize, Debug)]
pub struct MembershipDescribeRes {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub account_id: Uuid,
    pub project_id: Option<Uuid>,
    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub roles: Vec<Role>,
    pub tags: Vec<String>,
    pub meta: MembershipMeta,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

impl From<Membership> for MembershipDescribeRes {
    fn from(m: Membership) -> Self {
        Self {
            id: m.id,
            workspace_id: m.workspace_id,
            account_id: m.account.id,
            project_id: m.project_id,
            scope: m.scope,
            status: m.status,
            roles: m.roles,
            tags: m.tags,
            meta: m.meta,
            created_at: m.audit.created_at,
            updated_at: m.audit.updated_at,
        }
    }
}

// --- MembershipCreateReq ---
#[derive(Deserialize)]
pub struct MembershipCreateReq {
    pub account_id: Uuid,
    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub project_id: Option<Uuid>,
    pub role_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub meta: MembershipMeta,
}

impl IntoParams<MembershipCreateParams> for MembershipCreateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<MembershipCreateParams> {
        Ok(MembershipCreateParams {
            account_id: self.account_id,
            workspace_id,
            scope: self.scope,
            status: self.status,
            project_id: self.project_id,
            role_ids: self.role_ids,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- MembershipUpdateReq ---
#[derive(Deserialize)]
pub struct MembershipUpdateReq {
    pub id: Uuid,
    pub status: Option<MembershipStatus>,
    pub scope: Option<MembershipScope>,
    pub project_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<MembershipMeta>,
}

impl IntoParams<MembershipUpdateParams> for MembershipUpdateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<MembershipUpdateParams> {
        Ok(MembershipUpdateParams {
            id: self.id,
            workspace_id,
            status: self.status,
            scope: self.scope,
            project_id: self.project_id,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- MembershipListReq ---
#[derive(Deserialize, Debug)]
pub struct MembershipListReq {
    pub filter: Option<RequestFilterParams<MembershipFilter>>,
    pub options: Option<RequestListOptions>,
}

impl IntoParams<MembershipListParams> for MembershipListReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<MembershipListParams> {
        Ok(MembershipListParams {
            workspace_id,
            filter: self.filter,
            options: self.options,
        })
    }
}

// --- MembershipListRes ---
#[derive(Serialize, Debug)]
pub struct MembershipListRes {
    pub memberships: Vec<MembershipDescribeRes>,
    pub metadata: ListResponseMeta,
}

// --- MembershipDeleteReq ---
#[derive(Deserialize)]
pub struct MembershipDeleteReq {
    pub id: Uuid,
}

impl IntoParams<MembershipDeleteParams> for MembershipDeleteReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<MembershipDeleteParams> {
        Ok(MembershipDeleteParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- MembershipDeleteRes ---
#[derive(Serialize)]
pub struct MembershipDeleteRes {
    pub id: Uuid,
}

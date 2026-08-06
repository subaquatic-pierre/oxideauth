use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    membership::{
        Membership, MembershipCreateParams, MembershipDeleteParams, MembershipDescribeParams,
        MembershipFilter, MembershipListParams, MembershipMeta, MembershipUpdateParams,
    },
    role::Role,
};
use crate::store::entities::membership::{MembershipScope, MembershipStatus};

// --- MembershipDescribeReq ---
#[derive(Deserialize)]
pub struct MembershipDescribeReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

impl From<MembershipDescribeReq> for MembershipDescribeParams {
    fn from(value: MembershipDescribeReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
        }
    }
}

// --- MembershipDescribeRes ---
#[derive(Serialize, Debug)]
pub struct MembershipDescribeRes {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
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
            account_id: m.account.id,
            workspace_id: m.workspace.id,
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
    pub workspace_id: Uuid,
    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub project_id: Option<Uuid>,
    pub role_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub meta: MembershipMeta,
}

impl From<MembershipCreateReq> for MembershipCreateParams {
    fn from(value: MembershipCreateReq) -> Self {
        Self {
            account_id: value.account_id,
            workspace_id: value.workspace_id,
            scope: value.scope,
            status: value.status,
            project_id: value.project_id,
            role_ids: value.role_ids,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- MembershipUpdateReq ---
#[derive(Deserialize)]
pub struct MembershipUpdateReq {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub status: Option<MembershipStatus>,
    pub scope: Option<MembershipScope>,
    pub project_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<MembershipMeta>,
}

impl From<MembershipUpdateReq> for MembershipUpdateParams {
    fn from(value: MembershipUpdateReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
            status: value.status,
            scope: value.scope,
            project_id: value.project_id,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- MembershipListReq ---
#[derive(Deserialize, Debug)]
pub struct MembershipListReq {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<MembershipFilter>>,
    pub options: Option<RequestListOptions>,
}

impl From<MembershipListReq> for MembershipListParams {
    fn from(value: MembershipListReq) -> Self {
        Self {
            workspace_id: value.workspace_id,
            filter: value.filter,
            options: value.options,
        }
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
    pub workspace_id: Uuid,
}

impl From<MembershipDeleteReq> for MembershipDeleteParams {
    fn from(value: MembershipDeleteReq) -> Self {
        Self {
            id: value.id,
            workspace_id: value.workspace_id,
        }
    }
}

// --- MembershipDeleteRes ---
#[derive(Serialize)]
pub struct MembershipDeleteRes {
    pub id: Uuid,
}

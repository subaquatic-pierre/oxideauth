use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            account::Account,
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            role::{Role, RoleCheck},
            workspace::Workspace,
        },
        traits::{
            filter::{OpValAccountId, OpValWorkspaceId},
            list::RequestListParams,
        },
    },
    store::entities::membership::{
        MembershipFilter as StoreMembershipFilter, MembershipForCreate, MembershipForUpdate,
        MembershipMeta as StoreMembershipMeta, MembershipRow, MembershipScope, MembershipStatus,
        MembershipWithRoles,
    },
};

pub type MembershipMeta = StoreMembershipMeta;
pub type MembershipFilter = StoreMembershipFilter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Membership {
    pub id: Uuid,
    pub account: Account,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,

    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub roles: Vec<Role>,
    pub version: i64,

    pub tags: Vec<String>,
    pub meta: MembershipMeta,
    pub audit: CoreAuditFields,
}

impl From<(MembershipRow, Vec<Role>, Account)> for Membership {
    fn from((row, roles, account): (MembershipRow, Vec<Role>, Account)) -> Self {
        Self {
            id: row.id.into(),
            account,
            workspace_id: row.workspace_id,
            project_id: row.project_id,
            scope: row.scope,
            version: row.version,
            status: row.status,
            roles,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MembershipCreateParams {
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub project_id: Option<Uuid>,
    pub role_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub meta: MembershipMeta,
}

impl From<MembershipCreateParams> for MembershipForCreate {
    fn from(params: MembershipCreateParams) -> Self {
        Self {
            account_id: params.account_id,
            workspace_id: params.workspace_id,
            scope: params.scope,
            status: params.status,
            project_id: params.project_id,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MembershipDescribeParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize, Default)]
pub struct MembershipUpdateParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub status: Option<MembershipStatus>,
    pub scope: Option<MembershipScope>,
    pub project_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<MembershipMeta>,
}

impl From<MembershipUpdateParams> for MembershipForUpdate {
    fn from(value: MembershipUpdateParams) -> Self {
        Self {
            status: value.status,
            scope: value.scope,
            project_id: value.project_id,
            version: None,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

#[derive(Default)]
pub struct MembershipListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<MembershipFilter>>,
    pub options: Option<RequestListOptions>,
}

pub struct MembershipDeleteParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

impl RequestListParams<MembershipFilter> for MembershipListParams {
    fn filter(&self) -> Option<RequestFilterParams<MembershipFilter>> {
        self.filter.clone()
    }
    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

impl OpValWorkspaceId for MembershipFilter {
    fn get_workspace_id_opval(&self) -> Option<&modql::filter::OpValString> {
        self.workspace_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

impl OpValAccountId for MembershipFilter {
    fn get_account_id_opval(&self) -> Option<&modql::filter::OpValString> {
        self.account_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

impl Default for Membership {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            account: Account::default(),
            workspace_id: Uuid::nil(),
            project_id: None,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            version: 0,
            roles: vec![],
            tags: vec![],
            meta: MembershipMeta::default(),
            audit: CoreAuditFields::default(),
        }
    }
}

// Re-export the cache entity (migrated to `cache::entities::membership`,
// kept here for backward compatibility).
pub use crate::cache::entities::membership::MembershipCache;

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
        MembershipFilter as StoreMembershipFilter, MembershipForUpdate,
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
    pub workspace: Workspace,
    pub project_id: Option<Uuid>,

    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub roles: Vec<Role>,

    pub tags: Vec<String>,
    pub meta: MembershipMeta,
    pub audit: CoreAuditFields,
}

impl Membership {
    pub fn from_row_with_entities(
        membership: MembershipRow,
        roles: Vec<Role>,
        account: Account,
        workspace: Workspace,
    ) -> CoreResult<Self> {
        let row = membership;

        if Uuid::from(row.account_id) != account.id {
            return Err(CoreError::InvalidParams("Account ID mismatch".into()));
        }
        if Uuid::from(row.workspace_id) != workspace.id {
            return Err(CoreError::InvalidParams("Workspace ID mismatch".into()));
        }

        Ok(Self {
            id: row.id.into(),
            account,
            workspace,
            project_id: row.project_id,
            scope: row.scope,
            status: row.status,
            roles,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        })
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
            token_version: None,
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
            workspace: Workspace::default(),
            project_id: None,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            roles: vec![],
            tags: vec![],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
            audit: CoreAuditFields::default(),
        }
    }
}

#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct CachedMembership {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub role_ids: Vec<Uuid>,
    pub permissions: Vec<String>,
}

impl Default for CachedMembership {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            account_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            project_id: None,
            role_ids: vec![],
            // Usually defaults to empty to deny access unless explicitly populated
            permissions: vec![],
        }
    }
}

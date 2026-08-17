use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            account::Account,
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            policy::Policy,
            profile::ProfileMeta,
            role::Role,
            workspace::Workspace,
        },
        traits::{
            filter::{OpValAccountId, OpValWorkspaceId},
            list::RequestListParams,
        },
    },
    store::entities::{
        audit::AuditMeta,
        membership::{
            JoinedPolicyOnMembership, MembershipFilter as StoreMembershipFilter,
            MembershipForCreate, MembershipForUpdate, MembershipMeta as StoreMembershipMeta,
            MembershipRow, MembershipScope, MembershipStatus, MembershipWithRoles,
        },
    },
};

pub type MembershipMeta = StoreMembershipMeta;
pub type MembershipFilter = StoreMembershipFilter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Membership {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub profile_id: Option<Uuid>,
    pub project_id: Option<Uuid>,

    pub scope: MembershipScope,
    pub status: MembershipStatus,
    pub roles: Vec<Role>,
    pub policies: Vec<Policy>,
    pub version: i64,

    pub tags: Vec<String>,
    pub meta: MembershipMeta,
    pub audit: CoreAuditFields,
}

// TODO: change From<MembershipRow> for Membership
impl Membership {
    pub fn from_with_roles_and_policies(
        membership: MembershipRow,
        roles: Vec<Role>,
        policies: Vec<Policy>,
    ) -> Self {
        Self {
            id: membership.id.into(),
            account_id: membership.account_id,
            workspace_id: membership.workspace_id,
            profile_id: membership.profile_id,
            project_id: membership.project_id,
            scope: membership.scope,
            version: membership.version,
            status: membership.status,
            roles: roles,
            policies,
            tags: membership.tags,
            meta: membership.meta,
            audit: membership.audit.into(),
        }
    }
}

/// A membership paired with its resolved policies. Mirrors
/// `MembershipWithRoles`; used by `MembershipService::describe` to attach the
/// `membership_policy` join results.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MembershipWithPolicies {
    pub membership: Membership,
    pub policies: Vec<Policy>,
}

impl From<MembershipWithPolicies> for Membership {
    fn from(mwp: MembershipWithPolicies) -> Self {
        Self {
            policies: mwp.policies,
            ..mwp.membership
        }
    }
}

/// Converts a `membership_policy`-joined policy row into the core `Policy`
/// model. Mirrors `From<PolicyRow> for Policy` (see `src/core/models/policy.rs`).
impl From<JoinedPolicyOnMembership> for Policy {
    fn from(row: JoinedPolicyOnMembership) -> Self {
        Self {
            id: row.id.into(),
            workspace_id: row.workspace_id,
            name: row.name,
            effect: row.effect,
            principal_id: row.principal_id,
            actions: row.actions,
            resource: row.resource,
            constraint: row.constraint_expr,
            description: row.description,
            tags: row.tags,
            meta: row.meta,
            audit: CoreAuditFields {
                created_by: Uuid::nil(),
                created_at: row.created_at,
                updated_by: None,
                updated_at: row.updated_at,
                meta: AuditMeta::default(),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MembershipCreateParams {
    pub account_id: Option<Uuid>,
    pub email: String,
    pub workspace_id: Option<Uuid>,
    pub profile: Option<MembershipProfileDetails>,
    pub scope: MembershipScope,
    pub status: Option<MembershipStatus>,
    pub project_id: Option<Uuid>,
    pub role_ids: Vec<Uuid>,
    pub policy_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub meta: MembershipMeta,
}

/// Optional persona details applied when a NEW profile is created for the
/// membership (the resolved account may already exist; the profile is the
/// workspace-local identity).
#[derive(Debug, Deserialize, Clone, Default)]
pub struct MembershipProfileDetails {
    pub name: Option<String>,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub timezone: Option<String>,
    pub avatar_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProfileMeta>,
}

#[derive(Debug, Deserialize)]
pub struct MembershipDescribeParams {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MembershipUpdateParams {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub status: Option<MembershipStatus>,
    pub scope: Option<MembershipScope>,
    // TODO: ensure service method links or unlinks roles if Some
    pub role_ids: Option<Vec<Uuid>>,
    pub policy_ids: Option<Vec<Uuid>>,
    pub project_id: Option<Uuid>,
    pub profile_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<MembershipMeta>,
}

impl MembershipUpdateParams {
    pub fn into_store_params(self, version: i64) -> MembershipForUpdate {
        MembershipForUpdate {
            status: self.status,
            scope: self.scope,
            project_id: self.project_id,
            profile_id: self.profile_id,
            version: Some(version),
            tags: self.tags,
            meta: self.meta,
        }
    }
}

#[derive(Default)]
pub struct MembershipListParams {
    pub workspace_id: Option<Uuid>,
    pub filter: Option<RequestFilterParams<MembershipFilter>>,
    pub options: Option<RequestListOptions>,
}

pub struct MembershipDeleteParams {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
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
            account_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            profile_id: None,
            project_id: None,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            version: 0,
            roles: vec![],
            policies: vec![],
            tags: vec![],
            meta: MembershipMeta::default(),
            audit: CoreAuditFields::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::models::policy::PolicyEffect, core::traits::filter::OpValIsString,
        store::entities::audit::AuditFields,
    };
    use time::OffsetDateTime;

    fn make_row() -> MembershipRow {
        let id = Uuid::new_v4();
        MembershipRow {
            id: id.into(),
            account_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            profile_id: Some(Uuid::new_v4()),
            scope: MembershipScope::Project,
            status: MembershipStatus::Suspended,
            project_id: Some(Uuid::new_v4()),
            version: 3,
            tags: vec!["t1".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields {
                created_by: id.into(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_by: None,
                updated_at: None,
                meta: Default::default(),
            },
        }
    }

    #[test]
    fn test_membership_default() {
        let membership = Membership::default();
        assert_eq!(membership.scope, MembershipScope::Workspace);
        assert_eq!(membership.status, MembershipStatus::Active);
        assert_eq!(membership.version, 0);
        assert_eq!(membership.workspace_id, Uuid::nil());
        assert!(membership.project_id.is_none());
        assert!(membership.profile_id.is_none());
        assert!(membership.roles.is_empty());
        assert!(membership.policies.is_empty());
        assert!(membership.tags.is_empty());
        assert_eq!(membership.audit.created_by, Uuid::nil());
    }

    #[test]
    fn test_membership_create_params_shape() {
        let account_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let params = MembershipCreateParams {
            account_id: Some(account_id),
            email: "member@example.com".to_string(),
            workspace_id: Some(workspace_id),
            profile: None,
            scope: MembershipScope::Project,
            status: Some(MembershipStatus::Invited),
            project_id: Some(project_id),
            role_ids: vec![Uuid::new_v4()],
            policy_ids: vec![Uuid::new_v4()],
            tags: vec!["t".to_string()],
            meta: MembershipMeta {
                schema_version: "2".to_string(),
            },
        };

        // account_id is resolved by the service; email is the required identity.
        assert_eq!(params.account_id, Some(account_id));
        assert_eq!(params.email, "member@example.com");
        assert_eq!(params.workspace_id, Some(workspace_id));
        assert!(params.profile.is_none());
        assert_eq!(params.scope, MembershipScope::Project);
        assert_eq!(params.status, Some(MembershipStatus::Invited));
        assert_eq!(params.project_id, Some(project_id));
        assert_eq!(params.tags, vec!["t".to_string()]);
        assert_eq!(params.meta.schema_version, "2");
    }

    #[test]
    fn test_membership_update_params_into_store() {
        let params = MembershipUpdateParams {
            id: Uuid::new_v4(),
            workspace_id: Some(Uuid::new_v4()),
            status: Some(MembershipStatus::Active),
            scope: Some(MembershipScope::Workspace),
            role_ids: Some(vec![Uuid::new_v4()]),
            policy_ids: Some(vec![Uuid::new_v4()]),
            project_id: None,
            profile_id: Some(Uuid::new_v4()),
            tags: Some(vec!["t".to_string()]),
            meta: Some(MembershipMeta {
                schema_version: "3".to_string(),
            }),
        };

        let store = params.into_store_params(9);
        assert_eq!(store.status, Some(MembershipStatus::Active));
        assert_eq!(store.scope, Some(MembershipScope::Workspace));
        assert_eq!(store.version, Some(9));
        assert!(store.project_id.is_none());
        assert!(store.profile_id.is_some());
        assert_eq!(store.tags, Some(vec!["t".to_string()]));
        assert_eq!(store.meta.unwrap().schema_version, "3");
    }

    #[test]
    fn test_membership_with_policies_into_membership() {
        let mut membership = Membership::default();
        membership.id = Uuid::new_v4();
        let policies = vec![Policy::default()];

        let membership: Membership = MembershipWithPolicies {
            membership,
            policies,
        }
        .into();
        assert_eq!(membership.policies.len(), 1);
    }

    #[test]
    fn test_policy_from_joined_policy_on_membership() {
        use crate::store::entities::policy::{PolicyEffect as StorePolicyEffect, PolicyMeta};

        let id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();
        let joined = JoinedPolicyOnMembership {
            id: id.into(),
            workspace_id: ws_id,
            name: Some("self-update".to_string()),
            effect: StorePolicyEffect::Deny,
            principal_id: None,
            actions: vec!["membership:delete".to_string()],
            resource: "*".to_string(),
            constraint_expr: None,
            description: None,
            tags: vec![],
            meta: PolicyMeta {
                schema_version: "1".to_string(),
            },
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: None,
        };

        let policy: Policy = joined.into();
        assert_eq!(policy.id, id);
        assert_eq!(policy.workspace_id, ws_id);
        assert_eq!(policy.name.as_deref(), Some("self-update"));
        assert_eq!(policy.effect, PolicyEffect::Deny);
        assert_eq!(policy.actions, vec!["membership:delete".to_string()]);
        assert_eq!(policy.resource, "*");
        assert_eq!(policy.audit.created_at, OffsetDateTime::UNIX_EPOCH);
        // The policy table carries no created_by/updated_by columns.
        assert_eq!(policy.audit.created_by, Uuid::nil());
        assert!(policy.audit.updated_by.is_none());
    }

    #[test]
    fn test_membership_scope_and_status_round_trip() {
        // MembershipScope
        assert_eq!(MembershipScope::Workspace.to_string(), "workspace");
        assert_eq!(MembershipScope::Project.to_string(), "project");
        assert_eq!(
            "project".parse::<MembershipScope>().unwrap(),
            MembershipScope::Project
        );
        assert_eq!(
            "workspace".parse::<MembershipScope>().unwrap(),
            MembershipScope::Workspace
        );
        assert!("bogus".parse::<MembershipScope>().is_err());

        // MembershipStatus
        assert_eq!(MembershipStatus::Invited.to_string(), "invited");
        assert_eq!(MembershipStatus::Active.to_string(), "active");
        assert_eq!(MembershipStatus::Suspended.to_string(), "suspended");
        assert_eq!(
            "invited".parse::<MembershipStatus>().unwrap(),
            MembershipStatus::Invited
        );
        assert_eq!(
            "suspended".parse::<MembershipStatus>().unwrap(),
            MembershipStatus::Suspended
        );
        assert!("bogus".parse::<MembershipStatus>().is_err());
    }

    #[test]
    fn test_membership_list_params_accessors() {
        let params = MembershipListParams::default();
        assert!(params.filter().is_none());
        assert!(params.options().is_none());
    }

    #[test]
    fn test_membership_filter_opvals() {
        let filter = MembershipFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());
        assert!(filter.get_account_id_opval().is_none());

        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let filter: MembershipFilter = serde_json::from_value(serde_json::json!({
            "workspace_id": ws_id.to_string(),
            "account_id": account_id.to_string(),
        }))
        .expect("filter should deserialize");

        let ws = filter.get_workspace_id_opval().expect("ws present");
        let acct = filter.get_account_id_opval().expect("account present");
        assert_eq!(ws.as_eq_string(), Some(ws_id.to_string().as_str()));
        assert_eq!(acct.as_eq_string(), Some(account_id.to_string().as_str()));
    }
}

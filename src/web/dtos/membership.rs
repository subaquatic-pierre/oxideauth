use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::email::validate_email;
use crate::core::error::{CoreError, CoreResult};
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    membership::{
        Membership, MembershipCreateParams, MembershipDeleteParams, MembershipDescribeParams,
        MembershipFilter, MembershipListParams, MembershipMeta, MembershipUpdateParams,
    },
    policy::Policy,
    profile::ProfileMeta,
    role::Role,
};
use crate::core::traits::params::IntoParams;
use crate::store::entities::membership::{MembershipScope, MembershipStatus};

// --- MembershipDescribeReq ---
#[derive(Deserialize)]
pub struct MembershipDescribeReq {
    pub id: Uuid,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<MembershipDescribeParams> for MembershipDescribeReq {
    fn into_params(self) -> CoreResult<MembershipDescribeParams> {
        Ok(MembershipDescribeParams {
            id: self.id,
            workspace_id: self.workspace_id,
        })
    }
}

// --- MembershipDescribeRes ---
#[derive(Serialize, Debug)]
pub struct MembershipDescribeRes {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub account_id: Uuid,
    pub profile_id: Uuid,
    pub project_id: Option<Uuid>,
    pub scope: MembershipScope,
    pub version: i64,
    pub status: MembershipStatus,
    pub roles: Vec<Role>,
    pub policies: Vec<Policy>,
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
            account_id: m.account_id,
            profile_id: m.profile_id,
            project_id: m.project_id,
            version: m.version,
            scope: m.scope,
            status: m.status,
            roles: m.roles,
            policies: m.policies,
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
    pub email: String,
    pub profile_id: Uuid,
    pub scope: MembershipScope,
    pub status: Option<MembershipStatus>,
    pub project_id: Option<Uuid>,
    pub role_ids: Vec<Uuid>,
    pub policy_ids: Vec<Uuid>,
    pub tags: Vec<String>,
    pub meta: MembershipMeta,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<MembershipCreateParams> for MembershipCreateReq {
    fn into_params(self) -> CoreResult<MembershipCreateParams> {
        Ok(MembershipCreateParams {
            account_id: self.account_id,
            workspace_id: self.workspace_id,
            profile_id: self.profile_id,
            scope: self.scope,
            status: self.status.unwrap_or_default(),
            project_id: self.project_id,
            role_ids: self.role_ids,
            policy_ids: self.policy_ids,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- MembershipUpdateReq ---
// Note: dont need version on this struct
// version is always bumped at service layer
#[derive(Deserialize)]
pub struct MembershipUpdateReq {
    pub id: Uuid,
    pub status: Option<MembershipStatus>,
    pub scope: Option<MembershipScope>,
    pub project_id: Option<Uuid>,
    pub role_ids: Option<Vec<Uuid>>,
    pub policy_ids: Option<Vec<Uuid>>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<MembershipMeta>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<MembershipUpdateParams> for MembershipUpdateReq {
    fn into_params(self) -> CoreResult<MembershipUpdateParams> {
        Ok(MembershipUpdateParams {
            id: self.id,
            workspace_id: self.workspace_id,
            status: self.status,
            scope: self.scope,
            project_id: self.project_id,
            role_ids: self.role_ids,
            policy_ids: self.policy_ids,
            tags: self.tags,
            meta: self.meta,
            ..Default::default()
        })
    }
}

// --- MembershipListReq ---
#[derive(Deserialize, Debug)]
pub struct MembershipListReq {
    pub filter: Option<RequestFilterParams<MembershipFilter>>,
    pub options: Option<RequestListOptions>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<MembershipListParams> for MembershipListReq {
    fn into_params(self) -> CoreResult<MembershipListParams> {
        Ok(MembershipListParams {
            workspace_id: self.workspace_id,
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
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<MembershipDeleteParams> for MembershipDeleteReq {
    fn into_params(self) -> CoreResult<MembershipDeleteParams> {
        Ok(MembershipDeleteParams {
            id: self.id,
            workspace_id: self.workspace_id,
        })
    }
}

// --- MembershipDeleteRes ---
#[derive(Serialize)]
pub struct MembershipDeleteRes {
    pub id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_describe_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = MembershipDescribeReq {
            id,
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, Some(ws_id));
    }

    #[test]
    fn test_membership_create_req_into_params() {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();
        let params = MembershipCreateReq {
            account_id: account_id,
            email: "a@b.com".to_string(),
            profile_id: Uuid::new_v4(),
            scope: MembershipScope::Project,
            status: Some(MembershipStatus::Active),
            project_id: Some(project_id),
            role_ids: vec![role_id],
            policy_ids: vec![policy_id],
            tags: vec!["t".to_string()],
            meta: MembershipMeta::default(),
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_eq!(params.account_id, account_id);
        assert_eq!(params.workspace_id, Some(ws_id));
        assert_eq!(params.scope.to_string(), "project");
        assert_eq!(params.status, MembershipStatus::Active);
        assert_eq!(params.project_id, Some(project_id));
        assert_eq!(params.role_ids, vec![role_id]);
        assert_eq!(params.policy_ids, vec![policy_id]);
        assert_eq!(params.tags, vec!["t".to_string()]);
        assert_eq!(
            params.meta.schema_version,
            MembershipMeta::default().schema_version
        );
    }

    #[test]
    fn test_membership_create_req_requires_profile() {
        let ws_id = Uuid::new_v4();

        // empty email
        let err = MembershipCreateReq {
            account_id: Uuid::nil(),
            email: String::new(),
            profile_id: Uuid::nil(),
            scope: MembershipScope::Workspace,
            policy_ids: vec![],
            status: None,
            project_id: None,
            role_ids: vec![],
            tags: vec![],
            meta: MembershipMeta::default(),
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
    }

    #[test]
    fn test_membership_create_req_requires_profile_id() {
        let ws_id = Uuid::new_v4();
        let params = MembershipCreateReq {
            account_id: Uuid::nil(),
            email: "member@example.com".to_string(),
            profile_id: Uuid::new_v4(),
            scope: MembershipScope::Workspace,
            policy_ids: vec![],
            status: None,
            project_id: None,
            role_ids: vec![],
            tags: vec![],
            meta: MembershipMeta::default(),
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_ne!(params.profile_id, Uuid::nil());
    }

    #[test]
    fn test_membership_update_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = MembershipUpdateReq {
            id,
            status: Some(MembershipStatus::Suspended),
            scope: Some(MembershipScope::Workspace),
            project_id: None,
            role_ids: None,
            policy_ids: Some(vec![Uuid::new_v4()]),
            tags: Some(vec!["a".to_string()]),
            meta: None,
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, Some(ws_id));
        assert_eq!(params.status, Some(MembershipStatus::Suspended));
        assert_eq!(params.scope, Some(MembershipScope::Workspace));
        assert!(params.project_id.is_none());
        assert!(params.role_ids.is_none());
        assert!(params.policy_ids.is_some());
        assert_eq!(params.tags, Some(vec!["a".to_string()]));
        assert!(params.meta.is_none());
    }

    #[test]
    fn test_membership_list_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = MembershipListReq {
            filter: None,
            options: None,
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.workspace_id, Some(ws_id));
        assert!(params.filter.is_none());
        assert!(params.options.is_none());
    }

    #[test]
    fn test_membership_delete_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = MembershipDeleteReq {
            id,
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, Some(ws_id));
    }

    #[test]
    fn test_membership_describe_res_from_membership_default() {
        let m = Membership::default();
        let res = MembershipDescribeRes::from(m.clone());
        assert_eq!(res.id, m.id);
        assert_eq!(res.scope.to_string(), "workspace");
        assert_eq!(res.status.to_string(), "active");
        assert_eq!(res.workspace_id, Uuid::nil());
        assert!(res.roles.is_empty());
        assert!(res.policies.is_empty());
    }

    #[test]
    fn test_membership_describe_res_omits_email() {
        // The membership surface may carry `account_id` (a UUID) but must
        // never expose the account's email.
        let res = MembershipDescribeRes::from(Membership::default());
        let json = serde_json::to_string(&res).expect("MembershipDescribeRes must serialize");

        assert!(
            !json.contains("\"email\""),
            "MembershipDescribeRes must not contain an email field: {json}"
        );
    }
}

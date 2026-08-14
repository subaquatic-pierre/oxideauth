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
    pub version: i64,
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
            account_id: m.account_id,
            project_id: m.project_id,
            version: m.version,
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
// Note: dont need version on this struct
// version is always bumped at service layer
#[derive(Deserialize)]
pub struct MembershipUpdateReq {
    pub id: Uuid,
    pub status: Option<MembershipStatus>,
    pub scope: Option<MembershipScope>,
    pub project_id: Option<Uuid>,
    pub role_ids: Option<Vec<Uuid>>,
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
            role_ids: self.role_ids,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_describe_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = MembershipDescribeReq { id }.into_params(ws_id).unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
    }

    #[test]
    fn test_membership_create_req_into_params() {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let role_id = Uuid::new_v4();
        let params = MembershipCreateReq {
            account_id,
            scope: MembershipScope::Project,
            status: MembershipStatus::Active,
            project_id: Some(project_id),
            role_ids: vec![role_id],
            tags: vec!["t".to_string()],
            meta: MembershipMeta::default(),
        }
        .into_params(ws_id)
        .unwrap();

        assert_eq!(params.account_id, account_id);
        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.scope.to_string(), "project");
        assert_eq!(params.status.to_string(), "active");
        assert_eq!(params.project_id, Some(project_id));
        assert_eq!(params.role_ids, vec![role_id]);
        assert_eq!(params.tags, vec!["t".to_string()]);
        assert_eq!(
            params.meta.schema_version,
            MembershipMeta::default().schema_version
        );
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
            tags: Some(vec!["a".to_string()]),
            meta: None,
        }
        .into_params(ws_id)
        .unwrap();

        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.status, Some(MembershipStatus::Suspended));
        assert_eq!(params.scope, Some(MembershipScope::Workspace));
        assert!(params.project_id.is_none());
        assert!(params.role_ids.is_none());
        assert_eq!(params.tags, Some(vec!["a".to_string()]));
        assert!(params.meta.is_none());
    }

    #[test]
    fn test_membership_list_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = MembershipListReq {
            filter: None,
            options: None,
        }
        .into_params(ws_id)
        .unwrap();
        assert_eq!(params.workspace_id, ws_id);
        assert!(params.filter.is_none());
        assert!(params.options.is_none());
    }

    #[test]
    fn test_membership_delete_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = MembershipDeleteReq { id }.into_params(ws_id).unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
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
    }
}

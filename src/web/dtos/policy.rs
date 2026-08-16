use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    policy::{
        Policy, PolicyCreateParams, PolicyDeleteParams, PolicyDescribeParams, PolicyEffect,
        PolicyFilter, PolicyListParams, PolicyMeta, PolicyUpdateParams,
    },
};
use crate::core::traits::params::IntoParams;

// --- PolicyDescribeReq ---
#[derive(Deserialize)]
pub struct PolicyDescribeReq {
    pub id: Uuid,
}

impl IntoParams<PolicyDescribeParams> for PolicyDescribeReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PolicyDescribeParams> {
        Ok(PolicyDescribeParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- PolicyDescribeRes ---
#[derive(Serialize, Debug)]
pub struct PolicyDescribeRes {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub effect: PolicyEffect,
    pub principal_id: Option<Uuid>,
    pub actions: Vec<String>,
    pub resource: String,
    pub constraint: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PolicyMeta,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

impl From<Policy> for PolicyDescribeRes {
    fn from(policy: Policy) -> Self {
        Self {
            id: policy.id,
            workspace_id: policy.workspace_id,
            name: policy.name,
            effect: policy.effect,
            principal_id: policy.principal_id,
            actions: policy.actions,
            resource: policy.resource,
            constraint: policy.constraint,
            description: policy.description,
            tags: policy.tags,
            meta: policy.meta,
            created_at: policy.audit.created_at,
            updated_at: policy.audit.updated_at,
        }
    }
}

// --- PolicyCreateReq ---
#[derive(Deserialize)]
pub struct PolicyCreateReq {
    pub name: Option<String>,
    pub effect: PolicyEffect,
    pub principal_id: Option<Uuid>,
    pub actions: Vec<String>,
    pub resource: String,
    pub constraint: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PolicyMeta,
}

impl IntoParams<PolicyCreateParams> for PolicyCreateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PolicyCreateParams> {
        Ok(PolicyCreateParams {
            workspace_id,
            name: self.name,
            effect: self.effect,
            principal_id: self.principal_id,
            actions: self.actions,
            resource: self.resource,
            constraint: self.constraint,
            description: self.description,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- PolicyUpdateReq ---
#[derive(Deserialize)]
pub struct PolicyUpdateReq {
    pub id: Uuid,
    pub name: Option<String>,
    pub effect: Option<PolicyEffect>,
    pub principal_id: Option<Uuid>,
    pub actions: Option<Vec<String>>,
    pub resource: Option<String>,
    pub constraint: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<PolicyMeta>,
}

impl IntoParams<PolicyUpdateParams> for PolicyUpdateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PolicyUpdateParams> {
        Ok(PolicyUpdateParams {
            id: self.id,
            workspace_id,
            name: self.name,
            effect: self.effect,
            principal_id: self.principal_id,
            actions: self.actions,
            resource: self.resource,
            constraint: self.constraint,
            description: self.description,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- PolicyListReq ---
#[derive(Deserialize, Debug)]
pub struct PolicyListReq {
    pub filter: Option<RequestFilterParams<PolicyFilter>>,
    pub options: Option<RequestListOptions>,
}

impl IntoParams<PolicyListParams> for PolicyListReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PolicyListParams> {
        Ok(PolicyListParams {
            workspace_id,
            filter: self.filter,
            options: self.options,
        })
    }
}

// --- PolicyListRes ---
#[derive(Serialize, Debug)]
pub struct PolicyListRes {
    pub policies: Vec<PolicyDescribeRes>,
    pub metadata: ListResponseMeta,
}

// --- PolicyDeleteReq ---
#[derive(Deserialize)]
pub struct PolicyDeleteReq {
    pub id: Uuid,
}

impl IntoParams<PolicyDeleteParams> for PolicyDeleteReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<PolicyDeleteParams> {
        Ok(PolicyDeleteParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- PolicyDeleteRes ---
#[derive(Serialize)]
pub struct PolicyDeleteRes {
    pub id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_describe_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = PolicyDescribeReq { id }.into_params(ws_id).unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
    }

    #[test]
    fn test_policy_create_req_into_params() {
        let ws_id = Uuid::new_v4();
        let principal_id = Uuid::new_v4();
        let params = PolicyCreateReq {
            name: Some("self-update".to_string()),
            effect: PolicyEffect::Allow,
            principal_id: Some(principal_id),
            actions: vec!["membership:update".to_string(), "profile:update".to_string()],
            resource: "self".to_string(),
            constraint: Some("profile.account.id === user.id".to_string()),
            description: Some("Members may update their own profile".to_string()),
            tags: vec!["system".to_string()],
            meta: PolicyMeta::default(),
        }
        .into_params(ws_id)
        .unwrap();

        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.name.as_deref(), Some("self-update"));
        assert_eq!(params.effect, PolicyEffect::Allow);
        assert_eq!(params.principal_id, Some(principal_id));
        assert_eq!(
            params.actions,
            vec!["membership:update".to_string(), "profile:update".to_string()]
        );
        assert_eq!(params.resource, "self");
        assert_eq!(params.constraint.as_deref(), Some("profile.account.id === user.id"));
        assert_eq!(params.tags, vec!["system".to_string()]);
        assert_eq!(params.meta.schema_version, PolicyMeta::default().schema_version);
    }

    #[test]
    fn test_policy_update_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = PolicyUpdateReq {
            id,
            name: Some("renamed".to_string()),
            effect: Some(PolicyEffect::Deny),
            principal_id: None,
            actions: Some(vec!["membership:delete".to_string()]),
            resource: Some("*".to_string()),
            constraint: None,
            description: None,
            tags: Some(vec![]),
            meta: None,
        }
        .into_params(ws_id)
        .unwrap();

        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.name.as_deref(), Some("renamed"));
        assert_eq!(params.effect, Some(PolicyEffect::Deny));
        assert_eq!(params.actions, Some(vec!["membership:delete".to_string()]));
        assert_eq!(params.resource.as_deref(), Some("*"));
        assert!(params.constraint.is_none());
        assert_eq!(params.tags, Some(vec![]));
    }

    #[test]
    fn test_policy_list_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = PolicyListReq {
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
    fn test_policy_delete_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = PolicyDeleteReq { id }.into_params(ws_id).unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
    }

    #[test]
    fn test_policy_describe_res_from_policy_default() {
        let policy = Policy::default();
        let res = PolicyDescribeRes::from(policy.clone());
        assert_eq!(res.id, policy.id);
        assert_eq!(res.workspace_id, Uuid::nil());
        assert_eq!(res.effect, PolicyEffect::Allow);
        assert_eq!(res.actions, vec!["membership:update".to_string()]);
        assert_eq!(res.resource, "self");
        assert!(res.name.is_none());
        assert!(res.constraint.is_none());
        assert_eq!(res.meta.schema_version, "1");
    }
}

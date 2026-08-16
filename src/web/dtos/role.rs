use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    permission::Permission,
    policy::Policy,
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
    pub policies: Vec<Policy>,
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
            policies: role.policies,
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
    pub policy_ids: Vec<Uuid>,
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
            policy_ids: self.policy_ids,
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
    pub policy_ids: Option<Vec<Uuid>>,
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
            policy_ids: self.policy_ids,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_describe_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = RoleDescribeReq { id }.into_params(ws_id).unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
    }

    #[test]
    fn test_role_create_req_into_params() {
        let ws_id = Uuid::new_v4();
        let perm_id = Uuid::new_v4();
        let policy_id = Uuid::new_v4();
        let params = RoleCreateReq {
            name: "admin".to_string(),
            description: Some("Full access".to_string()),
            permission_ids: vec![perm_id],
            policy_ids: vec![policy_id],
            tags: vec!["system".to_string()],
            meta: RoleMeta::default(),
        }
        .into_params(ws_id)
        .unwrap();

        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.name, "admin");
        assert_eq!(params.description.as_deref(), Some("Full access"));
        assert_eq!(params.permission_ids, vec![perm_id]);
        assert_eq!(params.policy_ids, vec![policy_id]);
        assert_eq!(params.tags, vec!["system".to_string()]);
        assert_eq!(params.meta.schema_version, RoleMeta::default().schema_version);
    }

    #[test]
    fn test_role_update_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = RoleUpdateReq {
            id,
            name: Some("editor".to_string()),
            description: None,
            permission_ids: None,
            policy_ids: Some(vec![Uuid::new_v4()]),
            tags: Some(vec![]),
            meta: None,
        }
        .into_params(ws_id)
        .unwrap();

        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.name.as_deref(), Some("editor"));
        assert!(params.description.is_none());
        assert!(params.permission_ids.is_none());
        assert!(params.policy_ids.is_some());
        assert_eq!(params.tags, Some(vec![]));
    }

    #[test]
    fn test_role_list_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = RoleListReq {
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
    fn test_role_delete_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = RoleDeleteReq { id }.into_params(ws_id).unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
    }

    #[test]
    fn test_role_describe_res_from_role_default() {
        let role = Role::default();
        let res = RoleDescribeRes::from(role.clone());
        assert_eq!(res.id, role.id);
        assert_eq!(res.name, "New Role");
        assert_eq!(res.workspace_id, Uuid::nil());
        assert!(res.permissions.is_empty());
        assert!(res.policies.is_empty());
        assert_eq!(res.meta.schema_version, "1");
    }
}

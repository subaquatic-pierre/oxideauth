use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    permission::{
        Permission, PermissionCreateParams, PermissionDeleteParams, PermissionDescribeParams,
        PermissionFilter, PermissionListParams, PermissionMeta, PermissionUpdateParams,
    },
};
use crate::core::traits::params::IntoParams;

// --- PermissionDescribeReq ---
#[derive(Deserialize)]
pub struct PermissionDescribeReq {
    pub id: Option<Uuid>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<PermissionDescribeParams> for PermissionDescribeReq {
    fn into_params(self) -> CoreResult<PermissionDescribeParams> {
        Ok(PermissionDescribeParams {
            id: self.id,
            workspace_id: self.workspace_id,
        })
    }
}

// --- PermissionDescribeRes ---
#[derive(Serialize, Debug)]
pub struct PermissionDescribeRes {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub key: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PermissionMeta,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

impl From<Permission> for PermissionDescribeRes {
    fn from(perm: Permission) -> Self {
        Self {
            id: perm.id,
            workspace_id: perm.workspace_id,
            key: perm.key,
            label: perm.label,
            description: perm.description,
            tags: perm.tags,
            meta: perm.meta,
            created_at: perm.audit.created_at,
            updated_at: perm.audit.updated_at,
        }
    }
}

// --- PermissionCreateReq ---
#[derive(Deserialize)]
pub struct PermissionCreateReq {
    pub key: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: PermissionMeta,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<PermissionCreateParams> for PermissionCreateReq {
    fn into_params(self) -> CoreResult<PermissionCreateParams> {
        Ok(PermissionCreateParams {
            workspace_id: self.workspace_id,
            key: self.key,
            label: self.label,
            description: self.description,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- PermissionUpdateReq ---
#[derive(Deserialize)]
pub struct PermissionUpdateReq {
    pub id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<PermissionMeta>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<PermissionUpdateParams> for PermissionUpdateReq {
    fn into_params(self) -> CoreResult<PermissionUpdateParams> {
        Ok(PermissionUpdateParams {
            id: self.id,
            workspace_id: self.workspace_id,
            name: self.name,
            description: self.description,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- PermissionListReq ---
#[derive(Deserialize, Debug)]
pub struct PermissionListReq {
    pub filter: Option<RequestFilterParams<PermissionFilter>>,
    pub options: Option<RequestListOptions>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<PermissionListParams> for PermissionListReq {
    fn into_params(self) -> CoreResult<PermissionListParams> {
        Ok(PermissionListParams {
            workspace_id: self.workspace_id,
            filter: self.filter,
            options: self.options,
        })
    }
}

// --- PermissionListRes ---
#[derive(Serialize, Debug)]
pub struct PermissionListRes {
    pub permissions: Vec<PermissionDescribeRes>,
    pub metadata: ListResponseMeta,
}

// --- PermissionDeleteReq ---
#[derive(Deserialize)]
pub struct PermissionDeleteReq {
    pub id: Uuid,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<PermissionDeleteParams> for PermissionDeleteReq {
    fn into_params(self) -> CoreResult<PermissionDeleteParams> {
        Ok(PermissionDeleteParams {
            id: self.id,
            workspace_id: self.workspace_id,
        })
    }
}

// --- PermissionDeleteRes ---
#[derive(Serialize)]
pub struct PermissionDeleteRes {
    pub id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::CoreError;
    use crate::core::traits::params::ValidateParams;

    #[test]
    fn test_permission_describe_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = PermissionDescribeReq {
            id: Some(id),
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.id, Some(id));
        assert_eq!(params.workspace_id, Some(ws_id));
    }

    #[test]
    fn test_permission_describe_req_into_params_none_id() {
        let params = PermissionDescribeReq {
            id: None,
            workspace_id: Some(Uuid::new_v4()),
        }
        .into_params()
        .unwrap();
        assert!(params.id.is_none());
    }

    #[test]
    fn test_permission_describe_params_validate_rejects_missing_id() {
        let params = PermissionDescribeParams {
            id: None,
            workspace_id: Some(Uuid::new_v4()),
        };
        let err = params.validate().unwrap_err();
        assert!(
            matches!(err, CoreError::InvalidParams(msg) if msg == "Permission describe must contain id")
        );
    }

    #[test]
    fn test_permission_describe_params_validate_accepts_id() {
        let params = PermissionDescribeParams {
            id: Some(Uuid::new_v4()),
            workspace_id: Some(Uuid::new_v4()),
        };
        let validated = params.validate().unwrap();
        assert!(validated.id.is_some());
    }

    #[test]
    fn test_permission_create_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = PermissionCreateReq {
            key: "projects:read".to_string(),
            label: Some("Read projects".to_string()),
            description: Some("Read projects".to_string()),
            tags: vec!["system".to_string()],
            meta: PermissionMeta::default(),
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_eq!(params.workspace_id, Some(ws_id));
        assert_eq!(params.key, "projects:read");
        assert_eq!(params.description.as_deref(), Some("Read projects"));
        assert_eq!(params.tags, vec!["system".to_string()]);
        assert_eq!(
            params.meta.schema_version,
            PermissionMeta::default().schema_version
        );
    }

    #[test]
    fn test_permission_update_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = PermissionUpdateReq {
            id,
            name: Some("new-name".to_string()),
            description: None,
            tags: None,
            meta: None,
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, Some(ws_id));
        assert_eq!(params.name.as_deref(), Some("new-name"));
        assert!(params.description.is_none());
        assert!(params.meta.is_none());
    }

    #[test]
    fn test_permission_list_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = PermissionListReq {
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
    fn test_permission_delete_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = PermissionDeleteReq {
            id,
            workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, Some(ws_id));
    }
}

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::cache::entities::client_auth::ClientAuthCache;
use crate::core::error::CoreResult;
use crate::core::models::credential::CredentialConfig;
use crate::core::models::{
    credential::{
        Credential, CredentialDeleteParams, CredentialDescribeParams, CredentialFilter,
        CredentialListParams, CredentialMeta, CredentialUpdateParams,
    },
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
};
use crate::core::traits::params::IntoParams;
use crate::store::entities::credential::{CredentialKind, CredentialProvider, CredentialStatus};

// --- CredentialDescribeReq ---
#[derive(Deserialize)]
pub struct CredentialDescribeReq {
    pub id: Uuid,
    pub account_id: Uuid,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<CredentialDescribeParams> for CredentialDescribeReq {
    fn into_params(self) -> CoreResult<CredentialDescribeParams> {
        Ok(CredentialDescribeParams {
            id: self.id,
            account_id: self.account_id,
            workspace_id: self.workspace_id,
            provider_id: self.provider_id,
            email: self.email,
        })
    }
}

// --- CredentialDescribeRes ---
// SECURITY: secret field is NOT serialized (core model uses #[serde(skip_serializing)])
#[derive(Serialize, Debug)]
pub struct CredentialDescribeRes {
    pub id: Uuid,
    pub account_id: Uuid,
    pub kind: CredentialKind,
    pub provider: CredentialProvider,
    pub status: CredentialStatus,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_used_at: Option<OffsetDateTime>,
    pub tags: Vec<String>,
    pub meta: CredentialMeta,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

impl From<Credential> for CredentialDescribeRes {
    fn from(c: Credential) -> Self {
        Self {
            id: c.id,
            account_id: c.account_id,
            kind: c.kind,
            provider: c.provider,
            status: c.status,
            provider_id: c.provider_id,
            email: c.email,
            last_used_at: c.last_used_at,
            tags: c.tags,
            meta: c.meta,
            created_at: c.audit.created_at,
            updated_at: c.audit.updated_at,
        }
    }
}

// NOTE: No CredentialCreateReq — create is excluded per spec

// --- CredentialUpdateReq ---
#[derive(Deserialize)]
pub struct CredentialUpdateReq {
    pub id: Uuid,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    pub account_id: Uuid,
    pub kind: Option<CredentialKind>,
    pub provider: Option<CredentialProvider>,
    pub status: Option<CredentialStatus>,
    pub new_provider_id: Option<String>,
    pub new_email: Option<String>,
    pub secret: Option<String>,
    pub config: Option<CredentialConfig>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub last_used_at: Option<OffsetDateTime>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<CredentialMeta>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<CredentialUpdateParams> for CredentialUpdateReq {
    fn into_params(self) -> CoreResult<CredentialUpdateParams> {
        Ok(CredentialUpdateParams {
            id: self.id,
            provider_id: self.provider_id,
            email: self.email,
            account_id: self.account_id,
            workspace_id: self.workspace_id,
            kind: self.kind,
            config: self.config,
            provider: self.provider,
            status: self.status,
            new_provider_id: self.new_provider_id,
            new_email: self.new_email,
            secret: self.secret,
            expires_at: self.expires_at,
            last_used_at: self.last_used_at,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- CredentialListReq ---
#[derive(Deserialize, Debug)]
pub struct CredentialListReq {
    pub filter: Option<RequestFilterParams<CredentialFilter>>,
    pub options: Option<RequestListOptions>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<CredentialListParams> for CredentialListReq {
    fn into_params(self) -> CoreResult<CredentialListParams> {
        Ok(CredentialListParams {
            workspace_id: self.workspace_id,
            filter: self.filter,
            options: self.options,
        })
    }
}

// --- CredentialListRes ---
#[derive(Serialize, Debug)]
pub struct CredentialListRes {
    pub credentials: Vec<CredentialDescribeRes>,
    pub metadata: ListResponseMeta,
}

// --- CredentialDeleteReq ---
#[derive(Deserialize)]
pub struct CredentialDeleteReq {
    pub id: Uuid,
    pub account_id: Uuid,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

impl IntoParams<CredentialDeleteParams> for CredentialDeleteReq {
    fn into_params(self) -> CoreResult<CredentialDeleteParams> {
        Ok(CredentialDeleteParams {
            id: self.id,
            account_id: self.account_id,
            workspace_id: self.workspace_id,
            provider_id: self.provider_id,
            email: self.email,
        })
    }
}

// --- CredentialDeleteRes ---
#[derive(Serialize)]
pub struct CredentialDeleteRes {
    pub id: Uuid,
}

// --- CredentialAuthenticateReq ---
// Public (unauthenticated) request: a client presents its credential id + secret.
#[derive(Deserialize)]
pub struct CredentialAuthenticateReq {
    pub credential_id: Uuid,
    pub secret: String,
}

// --- CredentialAuthenticateRes ---
#[derive(Serialize, Debug)]
pub struct CredentialAuthenticateRes {
    pub credential_id: Uuid,
    pub membership_id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub roles: Vec<Uuid>,
    pub permissions: Vec<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
}

impl From<ClientAuthCache> for CredentialAuthenticateRes {
    fn from(c: ClientAuthCache) -> Self {
        Self {
            credential_id: c.credential_id,
            membership_id: c.membership_id,
            account_id: c.account_id,
            workspace_id: c.workspace_id,
            roles: c.roles,
            permissions: c.permissions,
            expires_at: c.expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_describe_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let params = CredentialDescribeReq {
            id,
            account_id,
            provider_id: Some("google-1".to_string()),
            email: Some("ada@example.com".to_string()),
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_eq!(params.id, id);
        assert_eq!(params.account_id, account_id);
        assert_eq!(params.workspace_id, Some(ws_id));
        assert_eq!(params.provider_id.as_deref(), Some("google-1"));
        assert_eq!(params.email.as_deref(), Some("ada@example.com"));
    }

    #[test]
    fn test_credential_describe_req_into_params_none_fields() {
        let ws_id = Uuid::new_v4();
        let params = CredentialDescribeReq {
            id: Uuid::new_v4(),
            account_id: Uuid::new_v4(),
            provider_id: None,
            email: None,
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert!(params.provider_id.is_none());
        assert!(params.email.is_none());
    }

    #[test]
    fn test_credential_update_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let params = CredentialUpdateReq {
            id,
            provider_id: Some("old-provider".to_string()),
            email: Some("old@example.com".to_string()),
            account_id,
            kind: Some(CredentialKind::OAuth),
            provider: Some(CredentialProvider::Google),
            status: Some(CredentialStatus::Active),
            new_provider_id: Some("new-provider".to_string()),
            new_email: Some("new@example.com".to_string()),
            secret: Some("rotated".to_string()),
            config: Some(CredentialConfig::default()),
            last_used_at: None,
            tags: Some(vec!["t".to_string()]),
            meta: Some(CredentialMeta::default()),
        expires_at: None, workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();

        assert_eq!(params.id, id);
        assert_eq!(params.account_id, account_id);
        assert_eq!(params.workspace_id, Some(ws_id));
        assert_eq!(
            params.kind.as_ref().map(|k| k.to_string()),
            Some("oauth".to_string())
        );
        assert_eq!(params.provider, Some(CredentialProvider::Google));
        assert_eq!(params.status, Some(CredentialStatus::Active));
        assert_eq!(params.new_provider_id.as_deref(), Some("new-provider"));
        assert_eq!(params.new_email.as_deref(), Some("new@example.com"));
        assert_eq!(params.secret.as_deref(), Some("rotated"));
        assert_eq!(params.tags, Some(vec!["t".to_string()]));
        assert_eq!(
            params.meta.unwrap().schema_version,
            CredentialMeta::default().schema_version
        );
    }

    #[test]
    fn test_credential_list_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = CredentialListReq {
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
    fn test_credential_delete_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let params = CredentialDeleteReq {
            id,
            account_id,
            provider_id: None,
            email: None,
        workspace_id: Some(ws_id),
        }
        .into_params()
        .unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.account_id, account_id);
        assert_eq!(params.workspace_id, Some(ws_id));
    }

    #[test]
    fn test_credential_describe_res_from_credential_default() {
        let res = CredentialDescribeRes::from(Credential::default());
        assert_eq!(res.id, Uuid::default());
        assert_eq!(res.kind.to_string(), "password");
        assert_eq!(res.provider, CredentialProvider::Local);
        assert_eq!(res.status, CredentialStatus::Pending);
        assert_eq!(res.account_id, Uuid::default());
    }
}

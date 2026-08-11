use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

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
}

impl IntoParams<CredentialDescribeParams> for CredentialDescribeReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<CredentialDescribeParams> {
        Ok(CredentialDescribeParams {
            id: self.id,
            account_id: self.account_id,
            workspace_id,
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
            account_id: c.account.id,
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
    pub last_used_at: Option<OffsetDateTime>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<CredentialMeta>,
}

impl IntoParams<CredentialUpdateParams> for CredentialUpdateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<CredentialUpdateParams> {
        Ok(CredentialUpdateParams {
            id: self.id,
            provider_id: self.provider_id,
            email: self.email,
            account_id: self.account_id,
            workspace_id,
            kind: self.kind,
            config: self.config,
            provider: self.provider,
            status: self.status,
            new_provider_id: self.new_provider_id,
            new_email: self.new_email,
            secret: self.secret,
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
}

impl IntoParams<CredentialListParams> for CredentialListReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<CredentialListParams> {
        Ok(CredentialListParams {
            workspace_id,
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
}

impl IntoParams<CredentialDeleteParams> for CredentialDeleteReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<CredentialDeleteParams> {
        Ok(CredentialDeleteParams {
            id: self.id,
            account_id: self.account_id,
            workspace_id,
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

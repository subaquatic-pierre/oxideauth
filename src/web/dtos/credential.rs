use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::models::{
    credential::{
        Credential, CredentialDeleteParams, CredentialDescribeParams, CredentialFilter,
        CredentialListParams, CredentialMeta, CredentialUpdateParams,
    },
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
};
use crate::store::entities::credential::{CredentialKind, CredentialProvider, CredentialStatus};

// --- CredentialDescribeReq ---
#[derive(Deserialize)]
pub struct CredentialDescribeReq {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub provider_id: Option<String>,
    pub email: Option<String>,
}

impl From<CredentialDescribeReq> for CredentialDescribeParams {
    fn from(value: CredentialDescribeReq) -> Self {
        Self {
            id: value.id,
            account_id: value.account_id,
            workspace_id: value.workspace_id,
            provider_id: value.provider_id,
            email: value.email,
        }
    }
}

// --- CredentialDescribeRes ---
// SECURITY: secret field is NOT serialized (core model uses #[serde(skip_serializing)])
#[derive(Serialize, Debug)]
pub struct CredentialDescribeRes {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
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
            workspace_id: c.workspace.id,
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
    pub workspace_id: Uuid,
    pub kind: Option<CredentialKind>,
    pub provider: Option<CredentialProvider>,
    pub status: Option<CredentialStatus>,
    pub new_provider_id: Option<String>,
    pub new_email: Option<String>,
    pub secret: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub last_used_at: Option<OffsetDateTime>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<CredentialMeta>,
}

impl From<CredentialUpdateReq> for CredentialUpdateParams {
    fn from(value: CredentialUpdateReq) -> Self {
        Self {
            id: value.id,
            provider_id: value.provider_id,
            email: value.email,
            account_id: value.account_id,
            workspace_id: value.workspace_id,
            kind: value.kind,
            provider: value.provider,
            status: value.status,
            new_provider_id: value.new_provider_id,
            new_email: value.new_email,
            secret: value.secret,
            last_used_at: value.last_used_at,
            tags: value.tags,
            meta: value.meta,
        }
    }
}

// --- CredentialListReq ---
#[derive(Deserialize, Debug)]
pub struct CredentialListReq {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<CredentialFilter>>,
    pub options: Option<RequestListOptions>,
}

impl From<CredentialListReq> for CredentialListParams {
    fn from(value: CredentialListReq) -> Self {
        Self {
            workspace_id: value.workspace_id,
            filter: value.filter,
            options: value.options,
        }
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
    pub workspace_id: Uuid,
    pub provider_id: Option<String>,
    pub email: Option<String>,
}

impl From<CredentialDeleteReq> for CredentialDeleteParams {
    fn from(value: CredentialDeleteReq) -> Self {
        Self {
            id: value.id,
            account_id: value.account_id,
            workspace_id: value.workspace_id,
            provider_id: value.provider_id,
            email: value.email,
        }
    }
}

// --- CredentialDeleteRes ---
#[derive(Serialize)]
pub struct CredentialDeleteRes {
    pub id: Uuid,
}

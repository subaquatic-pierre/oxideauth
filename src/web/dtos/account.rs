use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::account::AccountKind;
use crate::core::models::{
    account::{
        Account, AccountCreateParams, AccountDeleteParams, AccountDescribeParams, AccountFilter,
        AccountListParams, AccountMeta, AccountUpdateParams,
    },
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
};
use crate::core::traits::params::IntoParams;

// --- AccountDescribeReq ---
#[derive(Deserialize)]
pub struct AccountDescribeReq {
    pub email: Option<String>,
    pub id: Option<Uuid>,
}

// Implement IntoParams to convert Web Req to Core Param
// This simplifies the handler, especially for structs where fields change names/types.
impl IntoParams<AccountDescribeParams> for AccountDescribeReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<AccountDescribeParams> {
        Ok(AccountDescribeParams {
            email: self.email,
            id: self.id,
            workspace_id,
        })
    }
}

#[derive(Serialize)]
pub struct AccountDescribeRes {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,

    pub enabled: bool,
    pub verified: bool,

    pub tags: Vec<String>,
    pub meta: AccountMeta,

    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

// --- From<Account> for AccountDescribeRes ---
// Implement From to convert the Core Account entity to the Web Response DTO
impl From<Account> for AccountDescribeRes {
    fn from(acc: Account) -> Self {
        Self {
            id: acc.id,
            email: acc.email,
            name: acc.name,
            description: acc.description,
            avatar_url: acc.avatar_url,
            enabled: acc.enabled,
            verified: acc.verified,
            tags: acc.tags,
            meta: acc.meta,
            created_at: acc.audit.created_at,
            updated_at: acc.audit.updated_at,
        }
    }
}
// --- AccountCreateReq ---
// Add fields required by core/AccountCreateParams (name, description, etc.)
#[derive(Deserialize)]
pub struct AccountCreateReq {
    pub email: String,
    pub password: String,
    pub kind: Option<AccountKind>,
    pub enabled: Option<bool>,
    pub verified: Option<bool>,
    // Fields that map to core::AccountCreateParams
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub tags: Option<Vec<String>>,
    // Note: AccountMeta must be defined here if it is part of the request payload
    pub meta: Option<AccountMeta>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<AccountCreateParams> for AccountCreateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<AccountCreateParams> {
        Ok(AccountCreateParams {
            email: self.email,
            kind: self.kind.unwrap_or(AccountKind::User),
            password: self.password,
            name: self.name,
            description: self.description,
            avatar_url: self.avatar_url,
            tags: self.tags,
            enabled: self.enabled.unwrap_or_default(),
            verified: self.verified.unwrap_or_default(),
            meta: self.meta,
            workspace_id,
        })
    }
}

// Simplified version matching the Core Params exactly:
#[derive(Deserialize)]
pub struct AccountUpdateReq {
    // Identifier (one or both must be provided)
    pub email: Option<String>,
    pub id: Option<Uuid>,

    // Fields to Update (all fields here are Option<T> to represent 'patch')
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub enabled: Option<bool>,
    pub version: Option<i64>,
    pub verified: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<AccountMeta>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<AccountUpdateParams> for AccountUpdateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<AccountUpdateParams> {
        Ok(AccountUpdateParams {
            email: self.email,
            id: self.id,
            name: self.name,
            description: self.description,
            version: self.version,
            avatar_url: self.avatar_url,
            enabled: self.enabled,
            verified: self.verified,
            tags: self.tags,
            workspace_id,
            meta: self.meta,
        })
    }
}

// --- AccountListReq ---
#[derive(Deserialize, Debug)]
pub struct AccountListReq {
    // These typically contain nested structs for filtering and pagination/sorting
    pub filter: Option<RequestFilterParams<AccountFilter>>,
    pub options: Option<RequestListOptions>,
}

// Assuming you have a corresponding AccountListParams struct in your core layer:
// use crate::core::models::account::AccountListParams;

impl IntoParams<AccountListParams> for AccountListReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<AccountListParams> {
        Ok(AccountListParams {
            filter: self.filter,
            workspace_id,
            options: self.options,
        })
    }
}

// --- AccountListRes ---
#[derive(Serialize, Debug)]
pub struct AccountListRes {
    pub accounts: Vec<Account>,
    pub metadata: ListResponseMeta,
}

// --- AccountDeleteReq ---
#[derive(Deserialize)]
pub struct AccountDeleteReq {
    pub email: Option<String>,
    pub id: Option<Uuid>,
}

// Implement IntoParams<AccountDeleteParams> for AccountDeleteReq
impl IntoParams<AccountDeleteParams> for AccountDeleteReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<AccountDeleteParams> {
        Ok(AccountDeleteParams {
            email: self.email,
            id: self.id,
            workspace_id,
        })
    }
}

#[derive(Serialize)]
pub struct AccountDeleteRes {
    pub id: Uuid,
}

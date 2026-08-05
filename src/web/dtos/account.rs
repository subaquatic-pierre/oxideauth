use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::models::{
    account::{
        Account, AccountCreateParams, AccountDeleteParams, AccountDescribeParams, AccountFilter,
        AccountListParams, AccountMeta, AccountUpdateParams,
    },
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
};

// --- AccountDescribeReq ---
#[derive(Deserialize)]
pub struct AccountDescribeReq {
    pub email: Option<String>,
    pub id: Option<Uuid>,
    pub workspace_id: Uuid,
}

// Implement From to convert Web Req to Core Param
// This simplifies the handler, especially for structs where fields change names/types.
impl From<AccountDescribeReq> for AccountDescribeParams {
    fn from(value: AccountDescribeReq) -> Self {
        Self {
            email: value.email,
            id: value.id,
            workspace_id: value.workspace_id,
        }
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
    pub workspace_id: Uuid,
    // Fields that map to core::AccountCreateParams
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub tags: Option<Vec<String>>,
    // Note: AccountMeta must be defined here if it is part of the request payload
    pub meta: Option<AccountMeta>,
}

// Implement From to convert Web Req to Core Param
impl From<AccountCreateReq> for AccountCreateParams {
    fn from(value: AccountCreateReq) -> Self {
        Self {
            email: value.email,
            password: value.password,
            name: value.name,
            description: value.description,
            avatar_url: value.avatar_url,
            tags: value.tags,
            meta: value.meta,
            workspace_id: value.workspace_id,
        }
    }
}

// Simplified version matching the Core Params exactly:
#[derive(Deserialize)]
pub struct AccountUpdateReq {
    // Identifier (one or both must be provided)
    pub email: Option<String>,
    pub id: Option<Uuid>,
    pub workspace_id: Uuid,

    // Fields to Update (all fields here are Option<T> to represent 'patch')
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub enabled: Option<bool>,
    pub verified: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<AccountMeta>,
}

// Implement From to convert Web Req to Core Param
impl From<AccountUpdateReq> for AccountUpdateParams {
    fn from(value: AccountUpdateReq) -> Self {
        Self {
            email: value.email,
            id: value.id,
            name: value.name,
            description: value.description,
            avatar_url: value.avatar_url,
            enabled: value.enabled,
            verified: value.verified,
            tags: value.tags,
            workspace_id: value.workspace_id,
            meta: value.meta,
        }
    }
}

// --- AccountListReq ---
#[derive(Deserialize, Debug)]
pub struct AccountListReq {
    // These typically contain nested structs for filtering and pagination/sorting
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<AccountFilter>>,
    pub options: Option<RequestListOptions>,
}

// Assuming you have a corresponding AccountListParams struct in your core layer:
// use crate::core::models::account::AccountListParams;

impl From<AccountListReq> for AccountListParams {
    fn from(value: AccountListReq) -> Self {
        Self {
            filter: value.filter,
            workspace_id: value.workspace_id,
            options: value.options,
        }
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
    pub workspace_id: Uuid,
}

// Implement From<AccountDeleteReq> for AccountDeleteParams
impl From<AccountDeleteReq> for AccountDeleteParams {
    fn from(value: AccountDeleteReq) -> Self {
        Self {
            email: value.email,
            id: value.id,
            workspace_id: value.workspace_id,
        }
    }
}

#[derive(Serialize)]
pub struct AccountDeleteRes {
    pub id: Uuid,
}

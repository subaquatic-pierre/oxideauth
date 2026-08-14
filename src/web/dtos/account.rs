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
    fn into_params(self, _workspace_id: Uuid) -> CoreResult<AccountDescribeParams> {
        Ok(AccountDescribeParams {
            email: self.email,
            id: self.id,
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
    fn into_params(self, _workspace_id: Uuid) -> CoreResult<AccountCreateParams> {
        Ok(AccountCreateParams {
            email: self.email,
            kind: self.kind.unwrap_or(AccountKind::User),
            name: self.name,
            description: self.description,
            avatar_url: self.avatar_url,
            tags: self.tags,
            enabled: self.enabled.unwrap_or_default(),
            verified: self.verified.unwrap_or_default(),
            meta: self.meta,
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
    pub verified: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<AccountMeta>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<AccountUpdateParams> for AccountUpdateReq {
    fn into_params(self, _workspace_id: Uuid) -> CoreResult<AccountUpdateParams> {
        Ok(AccountUpdateParams {
            email: self.email,
            id: self.id,
            name: self.name,
            description: self.description,
            avatar_url: self.avatar_url,
            enabled: self.enabled,
            verified: self.verified,
            tags: self.tags,
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
    fn into_params(self, _workspace_id: Uuid) -> CoreResult<AccountListParams> {
        Ok(AccountListParams {
            filter: self.filter,
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
    fn into_params(self, _workspace_id: Uuid) -> CoreResult<AccountDeleteParams> {
        Ok(AccountDeleteParams {
            email: self.email,
            id: self.id,
        })
    }
}

#[derive(Serialize)]
pub struct AccountDeleteRes {
    pub id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_describe_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = AccountDescribeReq {
            email: Some("ada@example.com".to_string()),
            id: Some(id),
        }
        .into_params(ws_id)
        .unwrap();

        assert_eq!(params.id, Some(id));
        assert_eq!(params.email.as_deref(), Some("ada@example.com"));
    }

    #[test]
    fn test_account_describe_req_into_params_all_none() {
        let params = AccountDescribeReq {
            email: None,
            id: None,
        }
        .into_params(Uuid::new_v4())
        .unwrap();
        assert!(params.id.is_none());
        assert!(params.email.is_none());
    }

    #[test]
    fn test_account_create_req_into_params_defaults() {
        let params = AccountCreateReq {
            email: "ada@example.com".to_string(),
            password: "s3cret".to_string(),
            kind: None,
            enabled: None,
            verified: None,
            name: "Ada".to_string(),
            description: None,
            avatar_url: None,
            tags: None,
            meta: None,
        }
        .into_params(Uuid::new_v4())
        .unwrap();

        assert_eq!(params.email, "ada@example.com");
        assert_eq!(params.kind.to_string(), "user");
        assert!(!params.enabled);
        assert!(!params.verified);
        assert_eq!(params.name, "Ada");
        assert!(params.description.is_none());
        assert!(params.tags.is_none());
        assert!(params.meta.is_none());
    }

    #[test]
    fn test_account_create_req_into_params_with_values() {
        let params = AccountCreateReq {
            email: "svc@example.com".to_string(),
            password: "pw".to_string(),
            kind: Some(AccountKind::Service),
            enabled: Some(true),
            verified: Some(true),
            name: "Svc".to_string(),
            description: Some("d".to_string()),
            avatar_url: Some("http://x".to_string()),
            tags: Some(vec!["t1".to_string()]),
            meta: Some(AccountMeta {
                schema_version: "2".to_string(),
            }),
        }
        .into_params(Uuid::new_v4())
        .unwrap();

        assert_eq!(params.kind.to_string(), "service");
        assert!(params.enabled);
        assert!(params.verified);
        assert_eq!(params.description.as_deref(), Some("d"));
        assert_eq!(params.avatar_url.as_deref(), Some("http://x"));
        assert_eq!(params.tags, Some(vec!["t1".to_string()]));
        assert_eq!(params.meta.unwrap().schema_version, "2");
    }

    #[test]
    fn test_account_update_req_into_params() {
        let id = Uuid::new_v4();
        let params = AccountUpdateReq {
            email: Some("new@example.com".to_string()),
            id: Some(id),
            name: Some("Renamed".to_string()),
            description: None,
            avatar_url: None,
            enabled: Some(false),
            verified: None,
            tags: Some(vec![]),
            meta: None,
        }
        .into_params(Uuid::new_v4())
        .unwrap();

        assert_eq!(params.email.as_deref(), Some("new@example.com"));
        assert_eq!(params.id, Some(id));
        assert_eq!(params.name.as_deref(), Some("Renamed"));
        assert_eq!(params.enabled, Some(false));
        assert_eq!(params.verified, None);
        assert_eq!(params.tags, Some(vec![]));
    }

    #[test]
    fn test_account_list_req_into_params() {
        let params = AccountListReq {
            filter: None,
            options: None,
        }
        .into_params(Uuid::new_v4())
        .unwrap();
        assert!(params.filter.is_none());
        assert!(params.options.is_none());
    }

    #[test]
    fn test_account_delete_req_into_params() {
        let id = Uuid::new_v4();
        let params = AccountDeleteReq {
            email: None,
            id: Some(id),
        }
        .into_params(Uuid::new_v4())
        .unwrap();
        assert_eq!(params.id, Some(id));
        assert!(params.email.is_none());
    }

    #[test]
    fn test_account_describe_res_from_account_default() {
        let res = AccountDescribeRes::from(Account::default());
        assert_eq!(res.id, Uuid::default());
        assert_eq!(res.name, String::default());
        assert!(res.tags.is_empty());
    }
}

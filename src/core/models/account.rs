use modql::filter::{OpValString, OpValsString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
        },
        traits::{
            filter::{OpValIsString, OpValWorkspaceId},
            list::RequestListParams,
        },
    },
    store::entities::account::{
        AccountFilter as StoreAccountFilter, AccountForCreate, AccountForUpdate,
        AccountKind as StoreAccountKind, AccountMeta as StoreAccountMeta, AccountRow,
    },
    utils::id::id_or_string,
};

pub type AccountKind = StoreAccountKind;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Account {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub enabled: bool,
    pub verified: bool,
    pub version: i64,
    pub tags: Vec<String>,
    pub meta: AccountMeta,
    #[serde(flatten)]
    pub audit: CoreAuditFields,
}

impl From<AccountRow> for Account {
    fn from(value: AccountRow) -> Self {
        Self {
            id: value.id.into(),

            email: value.email,
            name: value.name,
            description: value.description,
            avatar_url: value.avatar_url,
            version: value.version,

            enabled: value.enabled,
            verified: value.verified,

            tags: value.tags,
            meta: value.meta,

            audit: value.audit.into(),
        }
    }
}

impl Default for Account {
    fn default() -> Self {
        Self {
            id: Uuid::default(), // Uuid::nil()
            email: String::default(),
            name: String::default(),
            description: Option::default(),
            avatar_url: Option::default(),
            enabled: bool::default(),  // false
            verified: bool::default(), // false
            tags: Vec::default(),
            version: 0,
            meta: AccountMeta::default(),
            audit: CoreAuditFields::default(),
        }
    }
}

#[derive(Default)]
pub struct AccountCreateParams {
    pub email: String,
    pub name: String,
    pub kind: AccountKind,

    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub enabled: bool,
    pub verified: bool,
    pub tags: Option<Vec<String>>,
    pub meta: Option<AccountMeta>,
}

impl From<AccountCreateParams> for AccountForCreate {
    fn from(value: AccountCreateParams) -> Self {
        AccountForCreate {
            email: value.email,
            name: value.name,
            description: value.description,
            avatar_url: value.avatar_url,
            kind: value.kind,
            enabled: value.enabled,
            verified: value.verified,
            tags: value.tags.unwrap_or_default(),
            meta: value.meta.unwrap_or_default(),
        }
    }
}

impl AccountDescribeParams {
    fn id_or_email(&self) -> CoreResult<String> {
        id_or_string(self.id, self.email.clone(), Some("ID or email required"))
    }
}

#[derive(Default)]
pub struct AccountDescribeParams {
    pub email: Option<String>,
    pub id: Option<Uuid>,
}

pub struct AccountDeleteParams {
    pub email: Option<String>,
    pub id: Option<Uuid>,
}

impl AccountDeleteParams {
    pub fn id_or_email(&self) -> CoreResult<String> {
        id_or_string(self.id, self.email.clone(), Some("ID or email required"))
    }
}

pub struct AccountUpdateParams {
    pub email: Option<String>,
    pub id: Option<Uuid>,

    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar_url: Option<String>,

    pub enabled: Option<bool>,
    pub verified: Option<bool>,

    pub tags: Option<Vec<String>>,
    pub meta: Option<AccountMeta>,
}

impl AccountUpdateParams {
    pub fn into_store_params(self, version: Option<i64>) -> AccountForUpdate {
        AccountForUpdate {
            // email: None, // email updates are intentionally blocked (see service notes)
            name: self.name,
            description: self.description,
            avatar_url: self.avatar_url,
            enabled: self.enabled,
            version: version,
            verified: self.verified,
            tags: self.tags,
            meta: self.meta,
        }
    }

    pub fn id_or_email(&self) -> CoreResult<String> {
        id_or_string(self.id, self.email.clone(), Some("ID or email required"))
    }
}
pub struct AccountListParams {
    pub filter: Option<RequestFilterParams<AccountFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<AccountFilter> for AccountListParams {
    fn filter(&self) -> Option<RequestFilterParams<AccountFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

pub type AccountMeta = StoreAccountMeta;
pub type AccountFilter = StoreAccountFilter;

impl OpValWorkspaceId for AccountFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::entities::audit::{AuditFields, AuditMeta};
    use time::OffsetDateTime;

    #[test]
    fn test_account_from_row() {
        let id = Uuid::new_v4();
        let row = AccountRow {
            id: id.into(),
            email: "user@example.com".to_string(),
            name: "Test User".to_string(),
            description: Some("desc".to_string()),
            avatar_url: Some("https://avatar".to_string()),
            kind: AccountKind::User,
            enabled: true,
            version: 7,
            verified: true,
            tags: vec!["t1".to_string()],
            meta: AccountMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields {
                created_by: id.into(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_by: Some(id.into()),
                updated_at: Some(OffsetDateTime::UNIX_EPOCH),
                meta: AuditMeta::default(),
            },
        };

        let account: Account = row.into();
        assert_eq!(account.id, id);
        assert_eq!(account.email, "user@example.com");
        assert_eq!(account.name, "Test User");
        assert_eq!(account.description.as_deref(), Some("desc"));
        assert_eq!(account.avatar_url.as_deref(), Some("https://avatar"));
        assert!(account.enabled);
        assert!(account.verified);
        assert_eq!(account.version, 7);
        assert_eq!(account.tags, vec!["t1".to_string()]);
        assert_eq!(account.meta.schema_version, "1");
        assert_eq!(account.audit.created_by, id);
        assert_eq!(account.audit.created_at, OffsetDateTime::UNIX_EPOCH);
        assert_eq!(account.audit.updated_by, Some(id));
        assert_eq!(account.audit.updated_at, Some(OffsetDateTime::UNIX_EPOCH));
    }

    #[test]
    fn test_account_default() {
        let account = Account::default();
        assert_eq!(account.id, Uuid::nil());
        assert_eq!(account.email, "");
        assert_eq!(account.name, "");
        assert!(account.description.is_none());
        assert!(account.avatar_url.is_none());
        assert!(!account.enabled);
        assert!(!account.verified);
        assert_eq!(account.version, 0);
        assert!(account.tags.is_empty());
        assert_eq!(account.audit.created_by, Uuid::nil());
    }

    #[test]
    fn test_account_create_params_into_store() {
        let params = AccountCreateParams {
            email: "a@b.com".to_string(),
            name: "Alice".to_string(),
            kind: AccountKind::Service,
            description: Some("desc".to_string()),
            avatar_url: None,
            enabled: true,
            verified: false,
            tags: Some(vec!["t1".to_string()]),
            meta: Some(AccountMeta {
                schema_version: "2".to_string(),
            }),
        };

        let store: AccountForCreate = params.into();
        assert_eq!(store.email, "a@b.com");
        assert_eq!(store.name, "Alice");
        assert_eq!(store.description.as_deref(), Some("desc"));
        assert!(store.avatar_url.is_none());
        assert_eq!(store.kind.to_string(), "service");
        assert!(store.enabled);
        assert!(!store.verified);
        assert_eq!(store.tags, vec!["t1".to_string()]);
        assert_eq!(store.meta.schema_version, "2");
    }

    #[test]
    fn test_account_create_params_defaults_tags_and_meta() {
        let params = AccountCreateParams {
            email: "a@b.com".to_string(),
            name: String::new(),
            kind: AccountKind::default(),
            description: None,
            avatar_url: None,
            enabled: false,
            verified: false,
            tags: None,
            meta: None,
        };

        let store: AccountForCreate = params.into();
        assert!(store.tags.is_empty());
        assert_eq!(store.meta.schema_version, "");
        assert_eq!(store.kind.to_string(), "user");
    }

    #[test]
    fn test_account_describe_params_id_or_email() {
        let params = AccountDescribeParams {
            email: None,
            id: None,
        };
        assert!(matches!(
            params.id_or_email().unwrap_err(),
            CoreError::InvalidParams(_)
        ));

        let id = Uuid::new_v4();
        let params = AccountDescribeParams {
            email: None,
            id: Some(id),
        };
        assert_eq!(params.id_or_email().unwrap(), id.to_string());

        let params = AccountDescribeParams {
            email: Some("a@b.com".to_string()),
            id: None,
        };
        assert_eq!(params.id_or_email().unwrap(), "a@b.com");

        // id wins when both are provided
        let params = AccountDescribeParams {
            email: Some("a@b.com".to_string()),
            id: Some(id),
        };
        assert_eq!(params.id_or_email().unwrap(), id.to_string());
    }

    #[test]
    fn test_account_delete_params_id_or_email() {
        let params = AccountDeleteParams {
            email: None,
            id: None,
        };
        assert!(params.id_or_email().is_err());

        let id = Uuid::new_v4();
        let params = AccountDeleteParams {
            email: None,
            id: Some(id),
        };
        assert_eq!(params.id_or_email().unwrap(), id.to_string());
    }

    #[test]
    fn test_account_update_params_id_or_email() {
        let params = AccountUpdateParams {
            email: None,
            id: None,
            name: None,
            description: None,
            avatar_url: None,
            enabled: None,
            verified: None,
            tags: None,
            meta: None,
        };
        assert!(params.id_or_email().is_err());

        let id = Uuid::new_v4();
        let params = AccountUpdateParams {
            email: Some("a@b.com".to_string()),
            id: Some(id),
            name: None,
            description: None,
            avatar_url: None,
            enabled: None,
            verified: None,
            tags: None,
            meta: None,
        };
        assert_eq!(params.id_or_email().unwrap(), id.to_string());
    }

    #[test]
    fn test_account_update_params_into_store() {
        let params = AccountUpdateParams {
            email: Some("new@b.com".to_string()),
            id: Some(Uuid::new_v4()),
            name: Some("New".to_string()),
            description: Some("d".to_string()),
            avatar_url: Some("u".to_string()),
            enabled: Some(false),
            verified: Some(true),
            tags: Some(vec!["t".to_string()]),
            meta: Some(AccountMeta {
                schema_version: "3".to_string(),
            }),
        };

        let store = params.into_store_params(Some(42));
        assert_eq!(store.name.as_deref(), Some("New"));
        assert_eq!(store.description.as_deref(), Some("d"));
        assert_eq!(store.avatar_url.as_deref(), Some("u"));
        assert_eq!(store.enabled, Some(false));
        assert_eq!(store.verified, Some(true));
        assert_eq!(store.version, Some(42));
        assert_eq!(store.tags, Some(vec!["t".to_string()]));
        assert_eq!(store.meta.unwrap().schema_version, "3");
    }

    #[test]
    fn test_account_list_params_accessors() {
        let params = AccountListParams {
            filter: None,
            options: None,
        };
        assert!(params.filter().is_none());
        assert!(params.options().is_none());
    }

    #[test]
    fn test_account_filter_workspace_id_opval_is_none() {
        let filter = AccountFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());
    }
}

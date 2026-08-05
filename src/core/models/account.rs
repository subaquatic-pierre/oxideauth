use modql::filter::{OpValString, OpValsString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
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
        AccountFilter as StoreAccountFilter, AccountMeta as StoreAccountMeta, AccountRow,
    },
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Account {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub enabled: bool,
    pub verified: bool,
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
            meta: AccountMeta::default(),
            audit: CoreAuditFields::default(),
        }
    }
}

#[derive(Default)]
pub struct AccountCreateParams {
    pub email: String,
    pub password: String,
    pub name: String,
    pub workspace_id: Uuid,

    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<AccountMeta>,
}

#[derive(Default)]
pub struct AccountDescribeParams {
    pub email: Option<String>,
    pub workspace_id: Uuid,
    pub id: Option<Uuid>,
}

pub struct AccountDeleteParams {
    pub workspace_id: Uuid,
    pub email: Option<String>,
    pub id: Option<Uuid>,
}

pub struct AccountUpdateParams {
    pub workspace_id: Uuid,
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
pub struct AccountListParams {
    pub workspace_id: Uuid,
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

use modql::filter::{OpValString, OpValsString};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            account::Account,
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            workspace::Workspace,
        },
        traits::{
            filter::{OpValAccountId, OpValWorkspaceId},
            list::RequestListParams,
        },
    },
    store::entities::credential::{
        CredentialConfig as StoreCredentialConfig, CredentialFilter as StoreCredentialFilter,
        CredentialForCreate, CredentialForUpdate, CredentialKind,
        CredentialMeta as StoreCredentialMeta, CredentialProvider, CredentialRow, CredentialStatus,
    },
};

pub type CredentialMeta = StoreCredentialMeta;
pub type CredentialFilter = StoreCredentialFilter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Credential {
    pub id: Uuid,

    pub account: Account,
    pub workspace: Workspace,

    pub kind: CredentialKind,
    pub provider: CredentialProvider,
    pub status: CredentialStatus,
    pub provider_id: Option<String>,
    pub email: Option<String>,

    #[serde(skip_serializing)]
    pub secret: Option<String>,

    pub last_used_at: Option<OffsetDateTime>,
    pub tags: Vec<String>,
    pub meta: CredentialMeta,
    pub config: CredentialConfig,

    pub audit: CoreAuditFields,
}

impl Default for Credential {
    fn default() -> Self {
        Self {
            id: Default::default(),
            account: Default::default(),
            workspace: Default::default(),
            kind: CredentialKind::Password,
            provider: CredentialProvider::Local,
            status: CredentialStatus::Pending,
            provider_id: Default::default(),
            email: Default::default(),
            secret: Default::default(),
            last_used_at: Default::default(),
            tags: Default::default(),
            config: CredentialConfig::default(),
            meta: Default::default(),
            audit: Default::default(),
        }
    }
}

impl Credential {
    pub fn from_row_with_entities(
        row: CredentialRow,
        account: Account,
        workspace: Workspace,
    ) -> CoreResult<Self> {
        if Uuid::from(row.account_id) != account.id {
            return Err(CoreError::InvalidParams(
                "row.account_id does not match account.id".to_string(),
            ));
        }
        if Uuid::from(row.workspace_id) != workspace.id {
            return Err(CoreError::InvalidParams(
                "row.workspace_id does not match workspace.id".to_string(),
            ));
        }

        Ok(Self {
            id: row.id.into(),
            account,
            workspace,
            kind: row.kind,
            provider: row.provider,
            status: row.status,
            provider_id: row.provider_id,
            email: row.email,
            secret: row.secret,
            config: row.config,
            last_used_at: row.last_used_at,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CredentialCreateParams {
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub kind: CredentialKind,
    pub provider: CredentialProvider,
    pub status: CredentialStatus,
    pub provider_id: Option<String>,
    pub email: Option<String>,
    pub secret: Option<String>,
    pub config: CredentialConfig,
    pub last_used_at: Option<OffsetDateTime>,
    pub tags: Vec<String>,
    pub meta: CredentialMeta,
}

impl From<CredentialCreateParams> for CredentialForCreate {
    fn from(params: CredentialCreateParams) -> Self {
        Self {
            kind: params.kind,
            provider: params.provider,
            status: params.status,
            account_id: params.account_id,
            workspace_id: params.workspace_id,
            provider_id: params.provider_id,
            email: params.email,
            secret: params.secret,
            config: params.config,
            last_used_at: params.last_used_at,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CredentialDescribeParams {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub provider_id: Option<String>,
    pub email: Option<String>,
}

impl CredentialDescribeParams {
    pub fn validate(&self) -> CoreResult<()> {
        Ok(())
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CredentialUpdateParams {
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
    pub last_used_at: Option<OffsetDateTime>,
    pub config: Option<CredentialConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<CredentialMeta>,
}

impl From<CredentialUpdateParams> for CredentialForUpdate {
    fn from(params: CredentialUpdateParams) -> Self {
        Self {
            kind: params.kind,
            provider: params.provider,
            status: params.status,
            provider_id: params.new_provider_id,
            email: params.new_email,
            config: params.config,
            secret: params.secret,
            last_used_at: params.last_used_at,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

pub type CredentialConfig = StoreCredentialConfig;

#[derive(Debug, Deserialize)]
pub struct CredentialDeleteParams {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub provider_id: Option<String>,
    pub email: Option<String>,
}

pub struct CredentialListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<CredentialFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<CredentialFilter> for CredentialListParams {
    fn filter(&self) -> Option<RequestFilterParams<CredentialFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

impl OpValWorkspaceId for CredentialFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        self.workspace_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

impl OpValAccountId for CredentialFilter {
    fn get_account_id_opval(&self) -> Option<&OpValString> {
        self.account_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

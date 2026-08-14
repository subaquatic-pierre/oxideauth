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

    pub account_id: Uuid,
    pub workspace_id: Uuid,

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
            account_id: Default::default(),
            workspace_id: Default::default(),
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

impl From<CredentialRow> for Credential {
    fn from(value: CredentialRow) -> Self {
        Self {
            id: value.id.into(),
            account_id: value.account_id.into(),
            workspace_id: value.workspace_id.into(),
            kind: value.kind,
            provider: value.provider,
            status: value.status,
            provider_id: value.provider_id,
            email: value.email,
            secret: value.secret,
            last_used_at: value.last_used_at,
            tags: value.tags,
            meta: value.meta,
            config: value.config,
            audit: value.audit.into(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{core::traits::filter::OpValIsString, store::entities::audit::AuditFields};

    fn make_row(account_id: Uuid, workspace_id: Uuid) -> CredentialRow {
        let id = Uuid::new_v4();
        CredentialRow {
            id: id.into(),
            account_id: account_id.into(),
            workspace_id: workspace_id.into(),
            kind: CredentialKind::ApiKey,
            provider: CredentialProvider::Github,
            status: CredentialStatus::Active,
            provider_id: Some("gh-1".to_string()),
            email: Some("u@x.com".to_string()),
            secret: Some("secret".to_string()),
            last_used_at: Some(OffsetDateTime::UNIX_EPOCH),
            config: CredentialConfig::default(),
            tags: vec!["t1".to_string()],
            meta: CredentialMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields {
                created_by: id.into(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_by: None,
                updated_at: None,
                meta: Default::default(),
            },
        }
    }

    #[test]
    fn test_credential_default() {
        let credential = Credential::default();
        assert_eq!(credential.id, Uuid::nil());
        assert_eq!(credential.account_id, Uuid::nil());
        assert_eq!(credential.kind.to_string(), "password");
        assert_eq!(credential.provider, CredentialProvider::Local);
        assert_eq!(credential.status, CredentialStatus::Pending);
        assert!(credential.provider_id.is_none());
        assert!(credential.email.is_none());
        assert!(credential.secret.is_none());
        assert!(credential.last_used_at.is_none());
        assert!(credential.tags.is_empty());
        assert_eq!(credential.audit.created_by, Uuid::nil());
    }

    #[test]
    fn test_credential_from_row_with_entities() {
        let account = Account::default(); // id = nil
        let workspace = Workspace::default(); // id = nil
        let row = make_row(Uuid::nil(), Uuid::nil());

        let credential: Credential = row.into();
        assert_eq!(credential.account_id, Uuid::nil());
        assert_eq!(credential.workspace_id, Uuid::nil());
        assert_eq!(credential.kind.to_string(), "api_key");
        assert_eq!(credential.provider, CredentialProvider::Github);
        assert_eq!(credential.status, CredentialStatus::Active);
        assert_eq!(credential.provider_id.as_deref(), Some("gh-1"));
        assert_eq!(credential.email.as_deref(), Some("u@x.com"));
        assert_eq!(credential.secret.as_deref(), Some("secret"));
        assert_eq!(credential.last_used_at, Some(OffsetDateTime::UNIX_EPOCH));
        assert_eq!(credential.tags, vec!["t1".to_string()]);
        assert_eq!(credential.meta.schema_version, "1");
        assert_eq!(credential.audit.created_at, OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn test_credential_create_params_into_store() {
        let account_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let params = CredentialCreateParams {
            account_id,
            workspace_id,
            kind: CredentialKind::OAuth,
            provider: CredentialProvider::Google,
            status: CredentialStatus::Pending,
            provider_id: Some("g-1".to_string()),
            email: Some("u@x.com".to_string()),
            secret: Some("s".to_string()),
            config: CredentialConfig::default(),
            last_used_at: None,
            tags: vec!["t".to_string()],
            meta: CredentialMeta {
                schema_version: "2".to_string(),
            },
        };

        let store: CredentialForCreate = params.into();
        assert_eq!(store.account_id, account_id);
        assert_eq!(store.workspace_id, workspace_id);
        assert_eq!(store.kind.to_string(), "oauth");
        assert_eq!(store.provider, CredentialProvider::Google);
        assert_eq!(store.status, CredentialStatus::Pending);
        assert_eq!(store.provider_id.as_deref(), Some("g-1"));
        assert_eq!(store.email.as_deref(), Some("u@x.com"));
        assert_eq!(store.secret.as_deref(), Some("s"));
        assert!(store.last_used_at.is_none());
        assert_eq!(store.tags, vec!["t".to_string()]);
        assert_eq!(store.meta.schema_version, "2");
    }

    #[test]
    fn test_credential_update_params_into_store() {
        let params = CredentialUpdateParams {
            id: Uuid::new_v4(),
            provider_id: Some("old".to_string()),
            email: Some("old@x.com".to_string()),
            account_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            kind: Some(CredentialKind::SSO),
            provider: None,
            status: Some(CredentialStatus::Revoked),
            new_provider_id: Some("new-provider".to_string()),
            new_email: Some("new@x.com".to_string()),
            secret: Some("s".to_string()),
            last_used_at: Some(OffsetDateTime::UNIX_EPOCH),
            config: None,
            tags: Some(vec!["t".to_string()]),
            meta: None,
        };

        let store: CredentialForUpdate = params.into();
        assert_eq!(store.kind.map(|k| k.to_string()), Some("sso".to_string()));
        assert!(store.provider.is_none());
        assert_eq!(store.status, Some(CredentialStatus::Revoked));
        // new_* fields override the old identifier fields
        assert_eq!(store.provider_id.as_deref(), Some("new-provider"));
        assert_eq!(store.email.as_deref(), Some("new@x.com"));
        assert_eq!(store.secret.as_deref(), Some("s"));
        assert_eq!(store.last_used_at, Some(OffsetDateTime::UNIX_EPOCH));
        assert_eq!(store.tags, Some(vec!["t".to_string()]));
        assert!(store.config.is_none());
        assert!(store.meta.is_none());
    }

    #[test]
    fn test_credential_describe_params_validate() {
        let params = CredentialDescribeParams {
            id: Uuid::new_v4(),
            account_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            provider_id: None,
            email: None,
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_credential_enums_round_trip() {
        // CredentialKind
        assert_eq!(CredentialKind::Password.to_string(), "password");
        assert_eq!(CredentialKind::OAuth.to_string(), "oauth");
        assert_eq!(CredentialKind::SSO.to_string(), "sso");
        assert_eq!(CredentialKind::ApiKey.to_string(), "api_key");
        assert!(matches!(
            "password".parse::<CredentialKind>().unwrap(),
            CredentialKind::Password
        ));
        assert!(matches!(
            "api_key".parse::<CredentialKind>().unwrap(),
            CredentialKind::ApiKey
        ));
        assert!("bogus".parse::<CredentialKind>().is_err());

        // CredentialProvider
        assert_eq!(CredentialProvider::Local.to_string(), "local");
        assert_eq!(CredentialProvider::Google.to_string(), "google");
        assert_eq!(CredentialProvider::Github.to_string(), "github");
        assert_eq!(
            "github".parse::<CredentialProvider>().unwrap(),
            CredentialProvider::Github
        );
        assert!("nope".parse::<CredentialProvider>().is_err());

        // CredentialStatus
        assert_eq!(CredentialStatus::Active.to_string(), "active");
        assert_eq!(CredentialStatus::Revoked.to_string(), "revoked");
        assert_eq!(CredentialStatus::Pending.to_string(), "pending");
        assert_eq!(
            "active".parse::<CredentialStatus>().unwrap(),
            CredentialStatus::Active
        );
        assert!("nope".parse::<CredentialStatus>().is_err());
    }

    #[test]
    fn test_credential_config_default() {
        let config = CredentialConfig::default();
        // jwt_max_age is private; verify via serialization round-trip stays stable
        assert!(serde_json::to_string(&config).is_ok());
    }

    #[test]
    fn test_credential_filter_opvals() {
        let filter = CredentialFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());
        assert!(filter.get_account_id_opval().is_none());

        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let filter: CredentialFilter = serde_json::from_value(serde_json::json!({
            "workspace_id": ws_id.to_string(),
            "account_id": account_id.to_string(),
        }))
        .expect("filter should deserialize");

        let ws = filter.get_workspace_id_opval().expect("ws present");
        let acct = filter.get_account_id_opval().expect("account present");
        assert_eq!(ws.as_eq_string(), Some(ws_id.to_string().as_str()));
        assert_eq!(acct.as_eq_string(), Some(account_id.to_string().as_str()));
    }
}

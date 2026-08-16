use modql::filter::OpValString;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    cache::entities::workspace::WorkspaceCache,
    core::{
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            workspace::Workspace,
        },
        traits::{filter::OpValWorkspaceId, list::RequestListParams},
    },
    store::entities::client::{
        ClientFilter as StoreClientFilter, ClientForCreate, ClientForUpdate,
        ClientMeta as StoreClientMeta, ClientRow,
    },
};

pub type ClientMeta = StoreClientMeta;
pub type ClientFilter = StoreClientFilter;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Client {
    pub id: Uuid,
    pub workspace_id: Uuid,

    pub name: String,
    pub endpoint: Option<String>,
    pub description: Option<String>,

    pub tags: Vec<String>,
    pub meta: ClientMeta,

    pub audit: CoreAuditFields,
}

impl Client {
    pub fn from_row_with_workspace(row: ClientRow, workspace: WorkspaceCache) -> Self {
        assert!(
            row.workspace_id == workspace.id,
            "row.workspace_id does not match workspace.id"
        );

        Self {
            id: row.id.into(),
            workspace_id: workspace.id,
            name: row.name,
            endpoint: row.endpoint,
            description: row.description,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        }
    }
}

impl Default for Client {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            name: "New Client".to_string(),
            endpoint: None,
            description: None,
            tags: vec![],
            meta: ClientMeta {
                schema_version: "1".to_string(),
            },
            audit: CoreAuditFields::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ClientCreateParams {
    pub workspace_id: Uuid,
    pub name: String,
    pub endpoint: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub meta: ClientMeta,
}

impl ClientCreateParams {
    /// Converts to store params. The `secret_hash` is generated
    /// by the service layer and never accepted from the client.
    pub fn into_store_params(self, secret_hash: String) -> ClientForCreate {
        ClientForCreate {
            workspace_id: self.workspace_id,
            name: self.name,
            secret_hash,
            endpoint: self.endpoint,
            description: self.description,
            tags: self.tags,
            meta: self.meta,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ClientUpdateParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ClientMeta>,
}

impl From<ClientUpdateParams> for ClientForUpdate {
    fn from(params: ClientUpdateParams) -> Self {
        Self {
            name: params.name,
            secret_hash: None, // secret rotation is handled via a dedicated endpoint
            endpoint: params.endpoint,
            description: params.description,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ClientDescribeParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

pub struct ClientDeleteParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

pub struct ClientValidateParams {
    pub workspace_id: Uuid,
    pub client_secret: String,
    pub user_token: String,
    pub required_permissions: Vec<String>,
}

pub struct ClientRegenerateSecretParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

pub struct ClientListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<ClientFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<ClientFilter> for ClientListParams {
    fn filter(&self) -> Option<RequestFilterParams<ClientFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

impl OpValWorkspaceId for ClientFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        self.workspace_id
            .as_ref()
            .and_then(|op_vals| op_vals.0.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cache::entities::workspace::WorkspaceCache,
        core::traits::filter::OpValIsString,
        store::entities::audit::{AuditFields, AuditMeta},
    };
    use time::OffsetDateTime;

    fn make_row(workspace_id: Uuid) -> ClientRow {
        let id = Uuid::new_v4();
        ClientRow {
            id: id.into(),
            workspace_id,
            name: "My Client".to_string(),
            secret_hash: "hash".to_string(),
            endpoint: Some("https://client.example.com".to_string()),
            description: Some("desc".to_string()),
            tags: vec!["t1".to_string()],
            meta: ClientMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields {
                created_by: id.into(),
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_by: None,
                updated_at: None,
                meta: AuditMeta::default(),
            },
        }
    }

    #[test]
    fn test_client_from_row_with_workspace() {
        let ws_id = Uuid::new_v4();
        let row = make_row(ws_id);

        let mut workspace = WorkspaceCache::default();
        workspace.id = ws_id;

        let client = Client::from_row_with_workspace(row, workspace);
        assert_eq!(client.workspace_id, ws_id);
        assert_eq!(client.name, "My Client");
        assert_eq!(client.endpoint.as_deref(), Some("https://client.example.com"));
        assert_eq!(client.description.as_deref(), Some("desc"));
        assert_eq!(client.tags, vec!["t1".to_string()]);
        assert_eq!(client.meta.schema_version, "1");
        assert_eq!(client.audit.created_at, OffsetDateTime::UNIX_EPOCH);
        assert!(client.audit.updated_by.is_none());
    }

    #[test]
    #[should_panic(expected = "row.workspace_id does not match workspace.id")]
    fn test_client_from_row_with_workspace_mismatch_panics() {
        let row = make_row(Uuid::new_v4());
        let workspace = WorkspaceCache::default(); // id = Uuid::nil()
        let _ = Client::from_row_with_workspace(row, workspace);
    }

    #[test]
    fn test_client_default() {
        let client = Client::default();
        assert_eq!(client.id, Uuid::nil());
        assert_eq!(client.workspace_id, Uuid::nil());
        assert_eq!(client.name, "New Client");
        assert!(client.endpoint.is_none());
        assert!(client.description.is_none());
        assert!(client.tags.is_empty());
        assert_eq!(client.meta.schema_version, "1");
        assert_eq!(client.audit.created_by, Uuid::nil());
    }

    #[test]
    fn test_client_create_params_into_store() {
        let ws_id = Uuid::new_v4();
        let params = ClientCreateParams {
            workspace_id: ws_id,
            name: "C".to_string(),
            endpoint: None,
            description: Some("d".to_string()),
            tags: vec!["x".to_string()],
            meta: ClientMeta {
                schema_version: "2".to_string(),
            },
        };

        let store = params.into_store_params("secret-hash".to_string());
        assert_eq!(store.workspace_id, ws_id);
        assert_eq!(store.name, "C");
        assert_eq!(store.secret_hash, "secret-hash");
        assert!(store.endpoint.is_none());
        assert_eq!(store.description.as_deref(), Some("d"));
        assert_eq!(store.tags, vec!["x".to_string()]);
        assert_eq!(store.meta.schema_version, "2");
    }

    #[test]
    fn test_client_update_params_into_store() {
        let params = ClientUpdateParams {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: Some("N".to_string()),
            endpoint: Some("e".to_string()),
            description: None,
            tags: Some(vec!["t".to_string()]),
            meta: Some(ClientMeta {
                schema_version: "3".to_string(),
            }),
        };

        let store: ClientForUpdate = params.into();
        assert_eq!(store.name.as_deref(), Some("N"));
        assert!(store.secret_hash.is_none(), "secret rotation is not part of update params");
        assert_eq!(store.endpoint.as_deref(), Some("e"));
        assert!(store.description.is_none());
        assert_eq!(store.tags, Some(vec!["t".to_string()]));
        assert_eq!(store.meta.unwrap().schema_version, "3");
    }

    #[test]
    fn test_client_list_params_accessors() {
        let params = ClientListParams {
            workspace_id: Uuid::new_v4(),
            filter: None,
            options: None,
        };
        assert!(params.filter().is_none());
        assert!(params.options().is_none());
    }

    #[test]
    fn test_client_filter_workspace_id_opval() {
        let filter = ClientFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());

        let ws_id = Uuid::new_v4();
        let filter: ClientFilter = serde_json::from_value(serde_json::json!({
            "workspace_id": ws_id.to_string()
        }))
        .expect("filter should deserialize");

        let opval = filter
            .get_workspace_id_opval()
            .expect("workspace_id should be present");
        let expected = ws_id.to_string();
        assert_eq!(opval.as_eq_string(), Some(expected.as_str()));
    }
}

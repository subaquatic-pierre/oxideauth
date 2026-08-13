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

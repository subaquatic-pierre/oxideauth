use modql::filter::{OpValString, OpValsString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    core::{
        error::{CoreError, CoreResult},
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            oath::AuthProvider,
        },
        traits::{
            filter::{OpValIsString, OpValWorkspaceId},
            list::RequestListParams,
        },
    },
    store::entities::workspace::{
        WorkspaceConfig as StoreWorkspaceConfig, WorkspaceFilter as StoreWorkspaceFilter,
        WorkspaceForCreate, WorkspaceForUpdate, WorkspaceMeta as StoreWorkspaceMeta, WorkspaceRow,
    },
    utils::id::id_or_string,
};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Workspace {
    pub id: Uuid,

    // Identity
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub owner: Uuid,

    // Config
    pub config: WorkspaceConfig,

    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,

    // Audit Fields (timestamps, creators, updaters)
    pub audit: CoreAuditFields,
}

pub type WorkspaceConfig = StoreWorkspaceConfig;

impl From<WorkspaceRow> for Workspace {
    fn from(row: WorkspaceRow) -> Self {
        // Assuming DbId can be directly converted to Uuid (common for SQLX/Postgres)
        let id: Uuid = row.id.into();

        let config = WorkspaceConfig::default();

        Self {
            id,
            name: row.name,
            slug: row.slug,
            description: row.description,
            owner: row.owner.into(),
            config: config,
            tags: row.tags,
            meta: row.meta,
            audit: row.audit.into(),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceCreateParams {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub owner: Option<Uuid>,
    pub config: WorkspaceConfig,
    pub tags: Vec<String>,
    pub meta: WorkspaceMeta,
}

impl WorkspaceCreateParams {
    pub fn into_store_params(self, owner: Uuid) -> WorkspaceForCreate {
        WorkspaceForCreate {
            name: self.name,
            slug: self.slug,
            description: self.description,
            owner: owner.into(),
            config: self.config.into(),
            tags: self.tags,
            meta: self.meta,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceListParams {
    pub filter: Option<RequestFilterParams<WorkspaceFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<WorkspaceFilter> for WorkspaceListParams {
    fn filter(&self) -> Option<RequestFilterParams<WorkspaceFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceDeleteParams {
    pub id: Option<Uuid>,
    pub slug: Option<String>,
}

impl WorkspaceDeleteParams {
    pub fn id_or_slug(&self) -> CoreResult<String> {
        id_or_string(self.id, self.slug.clone(), Some("ID or slug required"))
    }
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceUpdateParams {
    pub id: Option<Uuid>,
    pub slug: Option<String>,

    // Fields to Update (mirroring WorkspaceForUpdate)
    pub name: Option<String>,
    pub owner: Option<Uuid>,
    pub description: Option<String>,
    pub config: Option<WorkspaceConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<WorkspaceMeta>,
}

impl WorkspaceUpdateParams {
    pub fn id_or_slug(&self) -> CoreResult<String> {
        id_or_string(self.id, self.slug.clone(), Some("ID or slug required"))
    }
}

impl From<WorkspaceUpdateParams> for WorkspaceForUpdate {
    fn from(params: WorkspaceUpdateParams) -> Self {
        Self {
            name: params.name,
            slug: None, // slug in params is an identifier, not an update field
            owner: params.owner,
            description: params.description,
            config: params.config,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct WorkspaceDescribeParams {
    pub id: Option<Uuid>,
    pub slug: Option<String>,
}

impl WorkspaceDescribeParams {
    pub fn id_or_slug(&self) -> CoreResult<String> {
        id_or_string(self.id, self.slug.clone(), Some("ID or slug required"))
    }
}

pub type WorkspaceMeta = StoreWorkspaceMeta;
pub type WorkspaceFilter = StoreWorkspaceFilter;

impl OpValWorkspaceId for WorkspaceFilter {
    fn get_workspace_id_opval(&self) -> Option<&OpValString> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::entities::credential::DEFAULT_JWT_MAX_AGE;
    use time::OffsetDateTime;

    #[test]
    fn test_workspace_from_row() {
        let id = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let mut row = WorkspaceRow::default();
        row.id = id.into();
        row.name = "Acme".to_string();
        row.slug = "acme".to_string();
        row.description = Some("desc".to_string());
        row.owner = owner.into();
        row.tags = vec!["t1".to_string()];
        row.meta = WorkspaceMeta {
            schema_version: "2".to_string(),
        };
        // the row's config is intentionally overridden by the default in the conversion
        row.config = WorkspaceConfig {
            allowed_auth_providers: vec!["google".to_string()],
            jwt_max_age: 100,
            jwt_secret: "secret".to_string(),
            public: true,
        };

        let workspace: Workspace = row.into();
        assert_eq!(workspace.id, id);
        assert_eq!(workspace.name, "Acme");
        assert_eq!(workspace.slug, "acme");
        assert_eq!(workspace.description.as_deref(), Some("desc"));
        assert_eq!(workspace.owner, owner);
        assert_eq!(workspace.tags, vec!["t1".to_string()]);
        assert_eq!(workspace.meta.schema_version, "2");
        // config always falls back to the default
        assert_eq!(workspace.config.jwt_max_age, DEFAULT_JWT_MAX_AGE);
        assert_eq!(workspace.config.jwt_secret, "");
        assert!(!workspace.config.public);
        assert!(workspace.config.allowed_auth_providers.is_empty());
        assert_eq!(workspace.audit.created_by, Uuid::nil());
    }

    #[test]
    fn test_workspace_default() {
        let workspace = Workspace::default();
        assert_eq!(workspace.id, Uuid::nil());
        assert_eq!(workspace.name, "");
        assert_eq!(workspace.slug, "");
        assert_eq!(workspace.owner, Uuid::nil());
        assert!(workspace.description.is_none());
        assert!(workspace.tags.is_empty());
        assert_eq!(workspace.config.jwt_max_age, DEFAULT_JWT_MAX_AGE);
        assert_eq!(workspace.audit.created_at, OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn test_workspace_create_params_into_store() {
        let owner = Uuid::new_v4();
        let params = WorkspaceCreateParams {
            name: "Acme".to_string(),
            slug: "acme".to_string(),
            description: Some("d".to_string()),
            owner: None,
            config: WorkspaceConfig::default(),
            tags: vec!["t".to_string()],
            meta: WorkspaceMeta {
                schema_version: "1".to_string(),
            },
        };

        let store = params.into_store_params(owner);
        assert_eq!(store.name, "Acme");
        assert_eq!(store.slug, "acme");
        assert_eq!(store.description.as_deref(), Some("d"));
        assert_eq!(Uuid::from(store.owner), owner);
        assert_eq!(store.tags, vec!["t".to_string()]);
        assert_eq!(store.meta.schema_version, "1");
    }

    #[test]
    fn test_workspace_delete_params_id_or_slug() {
        let params = WorkspaceDeleteParams::default();
        assert!(matches!(
            params.id_or_slug().err().expect("both None should fail"),
            CoreError::InvalidParams(_)
        ));

        let id = Uuid::new_v4();
        let params = WorkspaceDeleteParams {
            id: Some(id),
            slug: None,
        };
        assert_eq!(params.id_or_slug().unwrap(), id.to_string());

        let params = WorkspaceDeleteParams {
            id: None,
            slug: Some("acme".to_string()),
        };
        assert_eq!(params.id_or_slug().unwrap(), "acme");

        // id wins when both provided
        let params = WorkspaceDeleteParams {
            id: Some(id),
            slug: Some("acme".to_string()),
        };
        assert_eq!(params.id_or_slug().unwrap(), id.to_string());
    }

    #[test]
    fn test_workspace_describe_params_id_or_slug() {
        let params = WorkspaceDescribeParams::default();
        assert!(params.id_or_slug().is_err());

        let params = WorkspaceDescribeParams {
            id: None,
            slug: Some("acme".to_string()),
        };
        assert_eq!(params.id_or_slug().unwrap(), "acme");
    }

    #[test]
    fn test_workspace_update_params_id_or_slug() {
        let params = WorkspaceUpdateParams::default();
        assert!(params.id_or_slug().is_err());

        let id = Uuid::new_v4();
        let params = WorkspaceUpdateParams {
            id: Some(id),
            slug: None,
            name: None,
            owner: None,
            description: None,
            config: None,
            tags: None,
            meta: None,
        };
        assert_eq!(params.id_or_slug().unwrap(), id.to_string());
    }

    #[test]
    fn test_workspace_update_params_into_store() {
        let params = WorkspaceUpdateParams {
            id: Some(Uuid::new_v4()),
            slug: Some("acme".to_string()),
            name: Some("New".to_string()),
            owner: Some(Uuid::new_v4()),
            description: Some("d".to_string()),
            config: Some(WorkspaceConfig::default()),
            tags: Some(vec!["t".to_string()]),
            meta: Some(WorkspaceMeta {
                schema_version: "2".to_string(),
            }),
        };

        let store: WorkspaceForUpdate = params.into();
        assert_eq!(store.name.as_deref(), Some("New"));
        // slug in params is an identifier, never an update field
        assert!(store.slug.is_none());
        assert!(store.owner.is_some());
        assert_eq!(store.description.as_deref(), Some("d"));
        assert!(store.config.is_some());
        assert_eq!(store.tags, Some(vec!["t".to_string()]));
        assert_eq!(store.meta.unwrap().schema_version, "2");
    }

    #[test]
    fn test_workspace_list_params_accessors() {
        let params = WorkspaceListParams::default();
        assert!(params.filter().is_none());
        assert!(params.options().is_none());
    }

    #[test]
    fn test_workspace_filter_workspace_id_opval_is_none() {
        let filter = WorkspaceFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());
    }
}

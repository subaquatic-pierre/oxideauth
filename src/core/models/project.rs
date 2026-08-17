use modql::filter::{ListOptions, OpValString, OpValsString, op_val_string};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    cache::entities::workspace::WorkspaceCache,
    core::{
        error::{CoreError, CoreResult},
        models::{
            audit::CoreAuditFields,
            list::{RequestFilterParams, RequestListOptions},
            workspace::Workspace,
        },
        traits::{
            filter::{OpValIsString, OpValWorkspaceId},
            list::RequestListParams,
            params::ValidateParams,
        },
    },
    store::{
        entities::project::{
            ProjectConfig as StoreProjectConfig, ProjectFilter as StoreProjectFilter,
            ProjectForCreate, ProjectForUpdate, ProjectMeta as StoreProjectMeta, ProjectRow,
        },
        utils::ListOptionsValidator,
    },
    utils::id::id_or_string,
};

impl From<ProjectCreateParams> for ProjectForCreate {
    fn from(params: ProjectCreateParams) -> Self {
        Self {
            workspace_id: params.workspace_id,
            name: params.name,
            code: params.code,
            description: params.description,
            owner: params.owner.unwrap_or_default().into(),
            config: params.config,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

impl From<ProjectUpdateParams> for ProjectForUpdate {
    fn from(params: ProjectUpdateParams) -> Self {
        Self {
            name: params.name,
            code: params.new_code,
            description: params.description,
            owner: None,
            config: params.config,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Project {
    pub id: Uuid,
    pub workspace_id: Uuid,

    // Project identity
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,

    // Config
    pub config: ProjectConfig,

    pub tags: Vec<String>,
    pub meta: ProjectMeta,

    // Audit Fields
    pub audit: CoreAuditFields,
}

impl From<ProjectRow> for Project {
    fn from(value: ProjectRow) -> Self {
        Self {
            id: value.id.into(),
            workspace_id: value.workspace_id,
            name: value.name,
            code: value.code,
            description: value.description,
            config: value.config,
            tags: value.tags,
            meta: value.meta,
            audit: value.audit.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProjectCreateParams {
    pub workspace_id: Option<Uuid>,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub owner: Option<Uuid>,

    pub config: ProjectConfig,
    pub tags: Vec<String>,
    pub meta: ProjectMeta,
}

#[derive(Debug, Deserialize)]
pub struct ProjectDescribeParams {
    pub id: Option<Uuid>,
    pub code: Option<String>,
    pub workspace_id: Option<Uuid>,
}

impl ValidateParams for ProjectDescribeParams {
    fn validate(self) -> CoreResult<Self> {
        // TODO: ensure params are correct
        Ok(self)
    }
}

impl ProjectDescribeParams {
    pub fn id_or_code(&self) -> CoreResult<String> {
        id_or_string(self.id, self.code.clone(), Some("ID or code required"))
    }
}

impl ProjectDeleteParams {
    pub fn id_or_code(&self) -> CoreResult<String> {
        id_or_string(self.id, self.code.clone(), Some("ID or code required"))
    }
}

impl ProjectUpdateParams {
    pub fn id_or_code(&self) -> CoreResult<String> {
        id_or_string(self.id, self.code.clone(), Some("ID or code required"))
    }
}

#[derive(Debug, Deserialize)]
pub struct ProjectUpdateParams {
    // Identifier
    pub id: Option<Uuid>,
    pub code: Option<String>,

    pub workspace_id: Option<Uuid>,
    pub name: Option<String>,
    pub new_code: Option<String>,
    pub description: Option<String>,
    pub config: Option<ProjectConfig>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProjectMeta>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectDeleteParams {
    pub id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub code: Option<String>,
}

pub struct ProjectListParams {
    pub workspace_id: Option<Uuid>,
    pub filter: Option<RequestFilterParams<ProjectFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<ProjectFilter> for ProjectListParams {
    fn filter(&self) -> Option<RequestFilterParams<ProjectFilter>> {
        self.filter.clone()
    }

    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

pub type ProjectConfig = StoreProjectConfig;
pub type ProjectMeta = StoreProjectMeta;
pub type ProjectFilter = StoreProjectFilter;

impl OpValWorkspaceId for ProjectFilter {
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

    fn make_row(workspace_id: Uuid) -> ProjectRow {
        let id = Uuid::new_v4();
        ProjectRow {
            id: id.into(),
            workspace_id,
            name: "Project Alpha".to_string(),
            code: Some("alpha".to_string()),
            description: Some("desc".to_string()),
            owner: Uuid::new_v4().into(),
            config: ProjectConfig {
                schema_version: "1".to_string(),
            },
            tags: vec!["t1".to_string()],
            meta: ProjectMeta {
                schema_version: "2".to_string(),
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
    fn test_project_default() {
        let project = Project::default();
        assert_eq!(project.id, Uuid::nil());
        assert_eq!(project.workspace_id, Uuid::nil());
        assert_eq!(project.name, "");
        assert!(project.code.is_none());
        assert!(project.description.is_none());
        assert!(project.tags.is_empty());
        assert_eq!(project.audit.created_by, Uuid::nil());
    }

    #[test]
    fn test_project_create_params_into_store() {
        let ws_id = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let params = ProjectCreateParams {
            workspace_id: Some(ws_id),
            name: "P".to_string(),
            code: Some("p1".to_string()),
            description: Some("d".to_string()),
            owner: Some(owner),
            config: ProjectConfig::default(),
            tags: vec!["t".to_string()],
            meta: ProjectMeta {
                schema_version: "1".to_string(),
            },
        };

        let store: ProjectForCreate = params.into();
        assert_eq!(store.workspace_id, Some(ws_id));
        assert_eq!(store.name, "P");
        assert_eq!(store.code.as_deref(), Some("p1"));
        assert_eq!(store.description.as_deref(), Some("d"));
        assert_eq!(Uuid::from(store.owner), owner);
        assert_eq!(store.tags, vec!["t".to_string()]);
        assert_eq!(store.meta.schema_version, "1");
    }

    #[test]
    fn test_project_create_params_defaults_owner() {
        let params = ProjectCreateParams {
            workspace_id: Some(Uuid::new_v4()),
            name: "P".to_string(),
            code: None,
            description: None,
            owner: None,
            config: ProjectConfig::default(),
            tags: vec![],
            meta: ProjectMeta::default(),
        };

        let store: ProjectForCreate = params.into();
        assert_eq!(Uuid::from(store.owner), Uuid::nil());
    }

    #[test]
    fn test_project_update_params_into_store() {
        let params = ProjectUpdateParams {
            id: Some(Uuid::new_v4()),
            code: Some("old-code".to_string()),
            workspace_id: Some(Uuid::new_v4()),
            name: Some("New".to_string()),
            new_code: Some("new-code".to_string()),
            description: Some("d".to_string()),
            config: Some(ProjectConfig::default()),
            tags: Some(vec!["t".to_string()]),
            meta: None,
        };

        let store: ProjectForUpdate = params.into();
        assert_eq!(store.name.as_deref(), Some("New"));
        // code is taken from new_code, not code (the identifier)
        assert_eq!(store.code.as_deref(), Some("new-code"));
        assert_eq!(store.description.as_deref(), Some("d"));
        assert!(store.owner.is_none(), "owner is never updated via params");
        assert!(store.config.is_some());
        assert_eq!(store.tags, Some(vec!["t".to_string()]));
        assert!(store.meta.is_none());
    }

    #[test]
    fn test_project_describe_params_validate() {
        let params = ProjectDescribeParams {
            id: Some(Uuid::new_v4()),
            code: Some("c".to_string()),
            workspace_id: Some(Uuid::new_v4()),
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_project_list_params_accessors() {
        let params = ProjectListParams {
            workspace_id: Some(Uuid::new_v4()),
            filter: None,
            options: None,
        };
        assert!(params.filter().is_none());
        assert!(params.options().is_none());
    }

    #[test]
    fn test_project_filter_workspace_id_opval() {
        let filter = ProjectFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());

        let ws_id = Uuid::new_v4();
        let filter: ProjectFilter = serde_json::from_value(serde_json::json!({
            "workspace_id": ws_id.to_string()
        }))
        .expect("filter should deserialize");

        let opval = filter.get_workspace_id_opval().expect("ws present");
        assert_eq!(opval.as_eq_string(), Some(ws_id.to_string().as_str()));
    }
}

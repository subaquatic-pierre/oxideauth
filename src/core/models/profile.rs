use modql::filter::OpValString;
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
            filter::OpValWorkspaceId,
            list::RequestListParams,
            params::ValidateParams,
        },
    },
    store::entities::profile::{
        ProfileFilter as StoreProfileFilter, ProfileForCreate, ProfileForUpdate,
        ProfileMeta as StoreProfileMeta, ProfileRow,
    },
};

pub type ProfileMeta = StoreProfileMeta;
pub type ProfileFilter = StoreProfileFilter;

impl From<ProfileCreateParams> for ProfileForCreate {
    fn from(params: ProfileCreateParams) -> Self {
        Self {
            account_id: params.account_id,
            workspace_id: params.workspace_id,
            name: params.name,
            description: params.description,
            display_name: params.display_name,
            job_title: params.job_title,
            timezone: params.timezone,
            avatar_url: params.avatar_url,
            tags: params.tags,
            meta: params.meta,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Profile {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,

    // Workspace-facing identity / presentation
    pub name: String,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub timezone: Option<String>,
    pub avatar_url: Option<String>,

    pub version: i64,
    pub tags: Vec<String>,
    pub meta: ProfileMeta,

    // Audit Fields
    pub audit: CoreAuditFields,
}

impl From<ProfileRow> for Profile {
    fn from(value: ProfileRow) -> Self {
        Self {
            id: value.id.into(),
            account_id: value.account_id,
            workspace_id: value.workspace_id,
            name: value.name,
            description: value.description,
            display_name: value.display_name,
            job_title: value.job_title,
            timezone: value.timezone,
            avatar_url: value.avatar_url,
            version: value.version,
            tags: value.tags,
            meta: value.meta,
            audit: value.audit.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProfileCreateParams {
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub timezone: Option<String>,
    pub avatar_url: Option<String>,
    pub tags: Vec<String>,
    pub meta: ProfileMeta,
}

#[derive(Debug, Deserialize)]
pub struct ProfileDescribeParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

impl ValidateParams for ProfileDescribeParams {
    fn validate(self) -> CoreResult<Self> {
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
pub struct ProfileDeleteParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize, Default)]
pub struct ProfileUpdateParams {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub timezone: Option<String>,
    pub avatar_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProfileMeta>,
}

impl ProfileUpdateParams {
    pub fn into_store_params(self, version: i64) -> ProfileForUpdate {
        ProfileForUpdate {
            name: self.name,
            description: self.description,
            display_name: self.display_name,
            job_title: self.job_title,
            timezone: self.timezone,
            avatar_url: self.avatar_url,
            version: Some(version),
            tags: self.tags,
            meta: self.meta,
        }
    }
}

#[derive(Default)]
pub struct ProfileListParams {
    pub workspace_id: Uuid,
    pub filter: Option<RequestFilterParams<ProfileFilter>>,
    pub options: Option<RequestListOptions>,
}

impl RequestListParams<ProfileFilter> for ProfileListParams {
    fn filter(&self) -> Option<RequestFilterParams<ProfileFilter>> {
        self.filter.clone()
    }
    fn options(&self) -> Option<RequestListOptions> {
        self.options.clone()
    }
}

impl OpValWorkspaceId for ProfileFilter {
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
        core::traits::filter::OpValIsString,
        store::entities::audit::{AuditFields, AuditMeta},
    };
    use time::OffsetDateTime;

    fn make_row(account_id: Uuid, workspace_id: Uuid) -> ProfileRow {
        let id = Uuid::new_v4();
        ProfileRow {
            id: id.into(),
            account_id,
            workspace_id,
            name: "Profile Alpha".to_string(),
            description: Some("desc".to_string()),
            display_name: Some("Alpha".to_string()),
            job_title: Some("Engineer".to_string()),
            timezone: Some("UTC".to_string()),
            avatar_url: Some("https://avatar.example/alice".to_string()),
            version: 3,
            tags: vec!["t1".to_string()],
            meta: ProfileMeta {
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
    fn test_profile_default() {
        let profile = Profile::default();
        assert_eq!(profile.id, Uuid::nil());
        assert_eq!(profile.account_id, Uuid::nil());
        assert_eq!(profile.workspace_id, Uuid::nil());
        assert_eq!(profile.name, "");
        assert!(profile.description.is_none());
        assert!(profile.display_name.is_none());
        assert!(profile.job_title.is_none());
        assert!(profile.timezone.is_none());
        assert!(profile.avatar_url.is_none());
        assert_eq!(profile.version, 0);
        assert!(profile.tags.is_empty());
        assert_eq!(profile.audit.created_by, Uuid::nil());
    }

    #[test]
    fn test_profile_from_row() {
        let account_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let profile = Profile::from(make_row(account_id, workspace_id));

        assert_eq!(profile.account_id, account_id);
        assert_eq!(profile.workspace_id, workspace_id);
        assert_eq!(profile.name, "Profile Alpha");
        assert_eq!(profile.display_name.as_deref(), Some("Alpha"));
        assert_eq!(profile.job_title.as_deref(), Some("Engineer"));
        assert_eq!(profile.timezone.as_deref(), Some("UTC"));
        assert_eq!(profile.version, 3);
        assert_eq!(profile.tags, vec!["t1".to_string()]);
        assert_eq!(profile.meta.schema_version, "2");
    }

    #[test]
    fn test_profile_create_params_into_store() {
        let account_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let params = ProfileCreateParams {
            account_id,
            workspace_id,
            name: "P".to_string(),
            description: Some("d".to_string()),
            display_name: Some("pn".to_string()),
            job_title: Some("jt".to_string()),
            timezone: Some("tz".to_string()),
            avatar_url: Some("av".to_string()),
            tags: vec!["t".to_string()],
            meta: ProfileMeta {
                schema_version: "1".to_string(),
            },
        };

        let store: ProfileForCreate = params.into();
        assert_eq!(store.account_id, account_id);
        assert_eq!(store.workspace_id, workspace_id);
        assert_eq!(store.name, "P");
        assert_eq!(store.description.as_deref(), Some("d"));
        assert_eq!(store.display_name.as_deref(), Some("pn"));
        assert_eq!(store.job_title.as_deref(), Some("jt"));
        assert_eq!(store.timezone.as_deref(), Some("tz"));
        assert_eq!(store.avatar_url.as_deref(), Some("av"));
        assert_eq!(store.tags, vec!["t".to_string()]);
        assert_eq!(store.meta.schema_version, "1");
    }

    #[test]
    fn test_profile_update_params_into_store() {
        let params = ProfileUpdateParams {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: Some("New".to_string()),
            description: Some("d".to_string()),
            display_name: Some("pn".to_string()),
            job_title: Some("jt".to_string()),
            timezone: Some("tz".to_string()),
            avatar_url: Some("av".to_string()),
            tags: Some(vec!["t".to_string()]),
            meta: None,
        };

        let store = params.into_store_params(9);
        assert_eq!(store.name.as_deref(), Some("New"));
        assert_eq!(store.description.as_deref(), Some("d"));
        assert_eq!(store.display_name.as_deref(), Some("pn"));
        assert_eq!(store.job_title.as_deref(), Some("jt"));
        assert_eq!(store.timezone.as_deref(), Some("tz"));
        assert_eq!(store.avatar_url.as_deref(), Some("av"));
        assert_eq!(store.version, Some(9));
        assert_eq!(store.tags, Some(vec!["t".to_string()]));
        assert!(store.meta.is_none());
    }

    #[test]
    fn test_profile_describe_params_validate() {
        let params = ProfileDescribeParams {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_profile_list_params_accessors() {
        let params = ProfileListParams {
            workspace_id: Uuid::new_v4(),
            filter: None,
            options: None,
        };
        assert!(params.filter().is_none());
        assert!(params.options().is_none());
    }

    #[test]
    fn test_profile_filter_workspace_id_opval() {
        let filter = ProfileFilter::default();
        assert!(filter.get_workspace_id_opval().is_none());

        let ws_id = Uuid::new_v4();
        let filter: ProfileFilter = serde_json::from_value(serde_json::json!({
            "workspace_id": ws_id.to_string()
        }))
        .expect("filter should deserialize");

        let opval = filter.get_workspace_id_opval().expect("ws present");
        assert_eq!(opval.as_eq_string(), Some(ws_id.to_string().as_str()));
    }
}

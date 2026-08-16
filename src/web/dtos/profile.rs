use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::error::CoreResult;
use crate::core::models::{
    list::{ListResponseMeta, RequestFilterParams, RequestListOptions},
    profile::{
        Profile, ProfileDeleteParams, ProfileDescribeParams, ProfileFilter, ProfileListParams,
        ProfileMeta, ProfileUpdateParams,
    },
};
use crate::core::traits::params::IntoParams;

// --- ProfileDescribeReq ---
#[derive(Deserialize)]
pub struct ProfileDescribeReq {
    pub id: Uuid,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<ProfileDescribeParams> for ProfileDescribeReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProfileDescribeParams> {
        Ok(ProfileDescribeParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- ProfileDescribeRes (and Update Response) ---
#[derive(Serialize, Debug)]
pub struct ProfileDescribeRes {
    pub id: Uuid,
    pub workspace_id: Uuid,
    // NOTE(email-privacy): account_id intentionally omitted from the response —
    // the workspace only ever sees the profile surface, never the account identity.

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
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
}

// --- From<Profile> for ProfileDescribeRes ---
// Implement From to convert the Core Profile entity to the Web Response DTO
impl From<Profile> for ProfileDescribeRes {
    fn from(profile: Profile) -> Self {
        Self {
            id: profile.id,
            workspace_id: profile.workspace_id,
            name: profile.name,
            description: profile.description,
            display_name: profile.display_name,
            job_title: profile.job_title,
            timezone: profile.timezone,
            avatar_url: profile.avatar_url,
            version: profile.version,
            tags: profile.tags,
            meta: profile.meta,
            created_at: profile.audit.created_at,
            updated_at: profile.audit.updated_at,
        }
    }
}

// --- ProfileUpdateReq ---
#[derive(Deserialize)]
pub struct ProfileUpdateReq {
    pub id: Uuid,

    // Fields to Update (all fields here are Option<T> to represent 'patch')
    pub name: Option<String>,
    pub description: Option<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub timezone: Option<String>,
    pub avatar_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<ProfileMeta>,
}

// Implement IntoParams to convert Web Req to Core Param
impl IntoParams<ProfileUpdateParams> for ProfileUpdateReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProfileUpdateParams> {
        Ok(ProfileUpdateParams {
            id: self.id,
            workspace_id,
            name: self.name,
            description: self.description,
            display_name: self.display_name,
            job_title: self.job_title,
            timezone: self.timezone,
            avatar_url: self.avatar_url,
            tags: self.tags,
            meta: self.meta,
        })
    }
}

// --- ProfileDeleteReq ---
#[derive(Deserialize)]
pub struct ProfileDeleteReq {
    pub id: Uuid,
}

impl IntoParams<ProfileDeleteParams> for ProfileDeleteReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProfileDeleteParams> {
        Ok(ProfileDeleteParams {
            id: self.id,
            workspace_id,
        })
    }
}

// --- ProfileDeleteRes ---
#[derive(Serialize)]
pub struct ProfileDeleteRes {
    pub id: Uuid,
}

// --- ProfileListReq ---
#[derive(Deserialize, Debug)]
pub struct ProfileListReq {
    // The filter and options are unchanged in structure but are mapped to ProfileFilter
    pub filter: Option<RequestFilterParams<ProfileFilter>>,
    pub options: Option<RequestListOptions>,
}

impl IntoParams<ProfileListParams> for ProfileListReq {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<ProfileListParams> {
        Ok(ProfileListParams {
            filter: self.filter,
            options: self.options,
            workspace_id,
        })
    }
}

// --- ProfileListRes ---
#[derive(Serialize, Debug)]
pub struct ProfileListRes {
    pub profiles: Vec<ProfileDescribeRes>,
    pub metadata: ListResponseMeta,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_describe_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = ProfileDescribeReq { id }.into_params(ws_id).unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
    }

    #[test]
    fn test_profile_update_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = ProfileUpdateReq {
            id,
            name: Some("renamed".to_string()),
            description: None,
            display_name: Some("display".to_string()),
            job_title: None,
            timezone: Some("UTC".to_string()),
            avatar_url: None,
            tags: None,
            meta: Some(ProfileMeta::default()),
        }
        .into_params(ws_id)
        .unwrap();

        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
        assert_eq!(params.name.as_deref(), Some("renamed"));
        assert_eq!(params.display_name.as_deref(), Some("display"));
        assert_eq!(params.timezone.as_deref(), Some("UTC"));
        assert!(params.meta.is_some());
    }

    #[test]
    fn test_profile_list_req_into_params() {
        let ws_id = Uuid::new_v4();
        let params = ProfileListReq {
            filter: None,
            options: None,
        }
        .into_params(ws_id)
        .unwrap();
        assert_eq!(params.workspace_id, ws_id);
        assert!(params.filter.is_none());
        assert!(params.options.is_none());
    }

    #[test]
    fn test_profile_describe_res_from_profile_default() {
        let profile = Profile::default();
        let res = ProfileDescribeRes::from(profile);
        assert_eq!(res.id, Uuid::default());
        assert_eq!(res.workspace_id, Uuid::default());
        assert_eq!(res.name, String::default());
        assert_eq!(res.meta.schema_version, ProfileMeta::default().schema_version);
    }

    #[test]
    fn test_profile_describe_res_omits_email_and_account_id() {
        // T018: the profile surface must never leak account identity — no
        // `email`, `account_id`, or `accountId` keys may appear in the JSON.
        let res = ProfileDescribeRes::from(Profile::default());
        let json = serde_json::to_string(&res).expect("ProfileDescribeRes must serialize");

        assert!(
            !json.contains("\"email\""),
            "ProfileDescribeRes must not contain an email field: {json}"
        );
        assert!(
            !json.contains("\"account_id\"") && !json.contains("\"accountId\""),
            "ProfileDescribeRes must not contain an account_id field: {json}"
        );
    }

    #[test]
    fn test_profile_delete_req_into_params() {
        let ws_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let params = ProfileDeleteReq { id }.into_params(ws_id).unwrap();
        assert_eq!(params.id, id);
        assert_eq!(params.workspace_id, ws_id);
    }
}

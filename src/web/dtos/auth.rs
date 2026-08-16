use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::models::{
    account::Account,
    auth::{
        ConfirmParams, LoginParams, OAuthInitiateParams, RefreshParams, RegisterParams,
        ResendConfirmParams, ResetPasswordParams, RevokeParams, UpdatePasswordParams,
    },
    workspace::WorkspaceDescribeParams,
};
use crate::web::dtos::account::AccountDescribeReq;
use crate::web::dtos::workspace::WorkspaceDescribeReq;

// --- AuthRegisterReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRegisterReq {
    pub email: String,
    pub password: Option<String>,
    pub name: Option<String>,
    /// The workspace to register into. Either `id` (UUID) or `slug` must be
    /// provided; when absent, the request-context workspace is used.
    pub workspace: WorkspaceDescribeReq,
}

impl From<AuthRegisterReq> for RegisterParams {
    fn from(req: AuthRegisterReq) -> Self {
        RegisterParams {
            email: req.email,
            password: req.password.unwrap_or_default(),
            name: req.name,
            workspace: req.workspace.into(),
        }
    }
}

// --- AuthRegisterRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRegisterRes {
    pub account: Account,
    pub access_token: String,
    pub refresh_token: String,
}

// --- AuthLoginReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthLoginReq {
    pub email: String,
    pub password: String,
    /// The workspace to log in to. Either `id` (UUID) or `slug` must be
    /// provided.
    pub workspace: WorkspaceDescribeReq,
}

impl From<AuthLoginReq> for LoginParams {
    fn from(req: AuthLoginReq) -> Self {
        LoginParams {
            email: req.email,
            password: req.password,
            workspace: req.workspace.into(),
        }
    }
}

// --- AuthLoginRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthLoginRes {
    pub account: Account,
    pub access_token: String,
    pub refresh_token: String,
}

// --- AuthRefreshReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRefreshReq {
    pub token: String,
}

impl From<AuthRefreshReq> for RefreshParams {
    fn from(req: AuthRefreshReq) -> Self {
        RefreshParams { token: req.token }
    }
}

// --- AuthRefreshRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRefreshRes {
    pub access_token: String,
    pub refresh_token: String,
}

// --- AuthResetPasswordReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResetPasswordReq {
    /// The account to request a password reset for. Either `email` or `id`
    /// must be provided.
    pub account: AccountDescribeReq,
}

impl From<AuthResetPasswordReq> for ResetPasswordParams {
    fn from(req: AuthResetPasswordReq) -> Self {
        ResetPasswordParams {
            account: req.account.into(),
        }
    }
}

// --- AuthResetPasswordRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResetPasswordRes {
    pub message: String,
}

// --- AuthUpdatePasswordReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUpdatePasswordReq {
    pub token: String,
    pub password: String,
}

impl From<AuthUpdatePasswordReq> for UpdatePasswordParams {
    fn from(req: AuthUpdatePasswordReq) -> Self {
        UpdatePasswordParams {
            token: req.token,
            new_password: req.password,
        }
    }
}

// --- AuthUpdatePasswordRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUpdatePasswordRes {
    pub account_id: Uuid,
}

// --- AuthConfirmAccountReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfirmAccountReq {
    pub token: String,
}

impl From<AuthConfirmAccountReq> for ConfirmParams {
    fn from(req: AuthConfirmAccountReq) -> Self {
        ConfirmParams { token: req.token }
    }
}

// --- AuthConfirmAccountRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfirmAccountRes {
    pub account_id: Uuid,
    pub verified: bool,
}

// --- AuthResendConfirmReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResendConfirmReq {
    /// The account to resend the confirmation for. Either `email` or `id`
    /// must be provided.
    pub account: AccountDescribeReq,
}

impl From<AuthResendConfirmReq> for ResendConfirmParams {
    fn from(req: AuthResendConfirmReq) -> Self {
        ResendConfirmParams {
            account: req.account.into(),
        }
    }
}

// --- AuthResendConfirmRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResendConfirmRes {
    pub message: String,
}

// --- AuthRevokeReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRevokeReq {
    pub token: String,
}

impl From<AuthRevokeReq> for RevokeParams {
    fn from(req: AuthRevokeReq) -> Self {
        RevokeParams { token: req.token }
    }
}

// --- AuthRevokeRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRevokeRes {
    pub revoked: bool,
}

// --- AuthOAuthInitiateReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthOAuthInitiateReq {
    pub redirect_url: String,
    pub workspace_id: Uuid,
}

impl From<AuthOAuthInitiateReq> for OAuthInitiateParams {
    fn from(req: AuthOAuthInitiateReq) -> Self {
        OAuthInitiateParams {
            redirect_url: req.redirect_url,
            workspace_id: req.workspace_id,
        }
    }
}

// --- AuthOAuthInitiateRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthOAuthInitiateRes {
    pub auth_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::CoreError;
    use crate::core::traits::params::ValidateParams;

    #[test]
    fn test_auth_register_req_to_register_params() {
        let params = RegisterParams::from(AuthRegisterReq {
            email: "ada@example.com".to_string(),
            password: Some("pw".to_string()),
            name: Some("Ada".to_string()),
            workspace: WorkspaceDescribeReq {
                id: None,
                slug: Some("ws-1".to_string()),
            },
        });
        assert_eq!(params.email, "ada@example.com");
        assert_eq!(params.password, "pw");
        assert_eq!(params.name.as_deref(), Some("Ada"));
        assert_eq!(params.workspace.slug.as_deref(), Some("ws-1"));
    }

    #[test]
    fn test_auth_register_req_missing_password_defaults_to_empty() {
        let params = RegisterParams::from(AuthRegisterReq {
            email: "ada@example.com".to_string(),
            password: None,
            name: None,
            workspace: WorkspaceDescribeReq {
                id: None,
                slug: Some("ws-1".to_string()),
            },
        });
        assert_eq!(params.password, "");
        assert!(params.name.is_none());
    }

    #[test]
    fn test_register_params_validate_trims_and_lowercases_email() {
        let params = RegisterParams {
            email: "  Ada@Example.COM ".to_string(),
            password: "pw".to_string(),
            name: None,
            workspace: WorkspaceDescribeParams {
                id: None,
                slug: Some("ws-1".to_string()),
            },
        }
        .validate()
        .unwrap();
        assert_eq!(params.email, "ada@example.com");
    }

    #[test]
    fn test_register_params_validate_missing_email() {
        let err = RegisterParams {
            email: "   ".to_string(),
            password: "pw".to_string(),
            name: None,
            workspace: WorkspaceDescribeParams {
                id: None,
                slug: Some("ws-1".to_string()),
            },
        }
        .validate()
        .err()
        .unwrap();
        assert!(matches!(err, CoreError::InvalidParams(msg) if msg == "email required"));
    }

    #[test]
    fn test_register_params_validate_missing_password() {
        let err = RegisterParams {
            email: "ada@example.com".to_string(),
            password: "".to_string(),
            name: None,
            workspace: WorkspaceDescribeParams {
                id: None,
                slug: Some("ws-1".to_string()),
            },
        }
        .validate()
        .err()
        .unwrap();
        assert!(matches!(err, CoreError::InvalidParams(msg) if msg == "password required"));
    }

    #[test]
    fn test_auth_login_req_to_login_params() {
        let params = LoginParams::from(AuthLoginReq {
            email: "ada@example.com".to_string(),
            password: "pw".to_string(),
            workspace: WorkspaceDescribeReq {
                id: None,
                slug: Some("ws-1".to_string()),
            },
        });
        assert_eq!(params.email, "ada@example.com");
        assert_eq!(params.password, "pw");
        assert!(params.workspace.id.is_none());
        assert_eq!(params.workspace.slug.as_deref(), Some("ws-1"));
    }

    #[test]
    fn test_auth_login_req_to_login_params_by_id() {
        let id = Uuid::new_v4();
        let params = LoginParams::from(AuthLoginReq {
            email: "ada@example.com".to_string(),
            password: "pw".to_string(),
            workspace: WorkspaceDescribeReq {
                id: Some(id),
                slug: None,
            },
        });
        assert_eq!(params.workspace.id, Some(id));
        assert!(params.workspace.slug.is_none());
    }

    #[test]
    fn test_auth_refresh_req_to_refresh_params() {
        let params = RefreshParams::from(AuthRefreshReq {
            token: "tok".to_string(),
        });
        assert_eq!(params.token, "tok");
    }

    #[test]
    fn test_auth_reset_password_req_to_reset_password_params() {
        let params = ResetPasswordParams::from(AuthResetPasswordReq {
            account: AccountDescribeReq {
                email: Some("ada@example.com".to_string()),
                id: None,
            },
        });
        assert_eq!(params.account.email.as_deref(), Some("ada@example.com"));
        assert!(params.account.id.is_none());
    }

    #[test]
    fn test_auth_update_password_req_to_update_password_params() {
        let params = UpdatePasswordParams::from(AuthUpdatePasswordReq {
            token: "tok".to_string(),
            password: "new-pw".to_string(),
        });
        assert_eq!(params.token, "tok");
        assert_eq!(params.new_password, "new-pw");
    }

    #[test]
    fn test_auth_confirm_account_req_to_confirm_params() {
        let params = ConfirmParams::from(AuthConfirmAccountReq {
            token: "tok".to_string(),
        });
        assert_eq!(params.token, "tok");
    }

    #[test]
    fn test_auth_resend_confirm_req_to_resend_confirm_params() {
        let params = ResendConfirmParams::from(AuthResendConfirmReq {
            account: AccountDescribeReq {
                email: None,
                id: Some(Uuid::new_v4()),
            },
        });
        assert!(params.account.email.is_none());
        assert!(params.account.id.is_some());
    }

    #[test]
    fn test_auth_revoke_req_to_revoke_params() {
        let params = RevokeParams::from(AuthRevokeReq {
            token: "tok".to_string(),
        });
        assert_eq!(params.token, "tok");
    }

    #[test]
    fn test_auth_oauth_initiate_req_to_oauth_initiate_params() {
        let ws_id = Uuid::new_v4();
        let params = OAuthInitiateParams::from(AuthOAuthInitiateReq {
            redirect_url: "http://localhost/callback".to_string(),
            workspace_id: ws_id,
        });
        assert_eq!(params.redirect_url, "http://localhost/callback");
        assert_eq!(params.workspace_id, ws_id);
    }
}

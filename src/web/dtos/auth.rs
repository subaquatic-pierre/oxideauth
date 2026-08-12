use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::models::{account::Account, auth::RegisterParams};

// --- AuthRegisterReq ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRegisterReq {
    pub email: String,
    pub password: Option<String>,
    pub name: Option<String>,
    /// The workspace to register into. Either `workspaceId` or `workspaceSlug`
    /// must be provided.
    pub workspace_id: String,
}

impl From<AuthRegisterReq> for RegisterParams {
    fn from(req: AuthRegisterReq) -> Self {
        RegisterParams {
            email: req.email,
            password: req.password.unwrap_or_default(),
            name: req.name,
            workspace_id: req.workspace_id,
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
    pub email: String,
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
    pub email: String,
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
}

// --- AuthOAuthInitiateRes ---
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthOAuthInitiateRes {
    pub auth_url: String,
}

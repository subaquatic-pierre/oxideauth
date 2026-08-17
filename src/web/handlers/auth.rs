use axum::{
    Json, Router,
    extract::{Extension, Query},
    response::Redirect,
    routing::{get, post},
};
use serde::Deserialize;
use tracing::debug;

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        models::auth::{
            ConfirmParams, LoginParams, OAuthCallbackParams, OAuthInitiateParams, RefreshParams,
            RegisterParams, ResendConfirmParams, ResetPasswordParams, RevokeParams,
            UpdatePasswordParams,
        },
    },
    web::{
        dtos::auth::{
            AuthConfirmAccountReq, AuthConfirmAccountRes, AuthLoginReq, AuthLoginRes,
            AuthOAuthInitiateReq, AuthOAuthInitiateRes, AuthRefreshReq, AuthRefreshRes,
            AuthRegisterReq, AuthRegisterRes, AuthResendConfirmReq, AuthResendConfirmRes,
            AuthResetPasswordReq, AuthResetPasswordRes, AuthRevokeReq, AuthRevokeRes,
            AuthUpdatePasswordReq, AuthUpdatePasswordRes,
        },
        error::{JsonReqResult, JsonResResult, WebError},
        response::WebResponse,
    },
};

// --- Register Account ---
#[axum::debug_handler]
pub async fn register(
    app: Extension<App>,
    body: JsonReqResult<AuthRegisterReq>,
) -> JsonResResult<WebResponse<AuthRegisterRes>> {
    let Json(body) = body?;
    let params: RegisterParams = body.into();
    let mut ctx = app.system_context()?;
    let svc = app.svc_reg.auth.clone();
    let result = svc.register(&mut ctx, params).await?;
    WebResponse::json(AuthRegisterRes {
        account: result.account,
        access_token: result.access_token,
        refresh_token: result.refresh_token,
    })
}

// --- Login (request token) ---
#[axum::debug_handler]
pub async fn login(
    app: Extension<App>,
    body: JsonReqResult<AuthLoginReq>,
) -> JsonResResult<WebResponse<AuthLoginRes>> {
    let Json(body) = body?;
    debug!("BODY: {body:?}");

    let params: LoginParams = body.into();
    let mut ctx = app.system_context()?;
    let svc = app.svc_reg.auth.clone();

    let result = svc.login(&mut ctx, params).await?;
    WebResponse::json(AuthLoginRes {
        account: result.account,
        access_token: result.access_token,
        refresh_token: result.refresh_token,
    })
}

// --- Refresh Token ---
#[axum::debug_handler]
pub async fn refresh(
    app: Extension<App>,
    body: JsonReqResult<AuthRefreshReq>,
) -> JsonResResult<WebResponse<AuthRefreshRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.auth.clone();
    let params: RefreshParams = body.into();
    let tp = svc.refresh_token(params).await?;
    WebResponse::json(AuthRefreshRes {
        access_token: tp.access_token,
        refresh_token: tp.refresh_token,
    })
}

// --- Reset Password (request reset email) ---
#[axum::debug_handler]
pub async fn reset_password(
    app: Extension<App>,
    body: JsonReqResult<AuthResetPasswordReq>,
) -> JsonResResult<WebResponse<AuthResetPasswordRes>> {
    let Json(body) = body?;
    let mut ctx = app.system_context()?;
    let svc = app.svc_reg.auth.clone();
    let params: ResetPasswordParams = body.into();
    svc.request_password_reset(&mut ctx, params).await?;
    WebResponse::json(AuthResetPasswordRes {
        message: "if the account exists, a password reset email has been sent".to_string(),
    })
}

// --- Update Password (confirm password reset) ---
#[axum::debug_handler]
pub async fn update_password(
    app: Extension<App>,
    body: JsonReqResult<AuthUpdatePasswordReq>,
) -> JsonResResult<WebResponse<AuthUpdatePasswordRes>> {
    let Json(body) = body?;
    let mut ctx = app.system_context()?;
    let svc = app.svc_reg.auth.clone();
    let params: UpdatePasswordParams = body.into();
    let account_id = svc.update_password(&mut ctx, params).await?;
    WebResponse::json(AuthUpdatePasswordRes { account_id })
}

// --- Confirm Account ---
#[axum::debug_handler]
pub async fn confirm_account(
    app: Extension<App>,
    body: JsonReqResult<AuthConfirmAccountReq>,
) -> JsonResResult<WebResponse<AuthConfirmAccountRes>> {
    let Json(body) = body?;
    let mut ctx = app.system_context()?;
    let svc = app.svc_reg.auth.clone();
    let params: ConfirmParams = body.into();
    let result = svc.confirm_account(&mut ctx, params).await?;
    WebResponse::json(AuthConfirmAccountRes {
        account_id: result.account_id,
        verified: result.was_already_verified,
    })
}

// --- Resend Confirmation ---
#[axum::debug_handler]
pub async fn resend_confirm(
    app: Extension<App>,
    body: JsonReqResult<AuthResendConfirmReq>,
) -> JsonResResult<WebResponse<AuthResendConfirmRes>> {
    let Json(body) = body?;
    let mut ctx = app.system_context()?;
    let svc = app.svc_reg.auth.clone();
    let params: ResendConfirmParams = body.into();
    svc.resend_confirmation(&mut ctx, params).await?;
    WebResponse::json(AuthResendConfirmRes {
        message: "if the account exists, a confirmation email has been sent".to_string(),
    })
}

// --- Revoke Token ---
#[axum::debug_handler]
pub async fn revoke(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<AuthRevokeReq>,
) -> JsonResResult<WebResponse<AuthRevokeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.auth.clone();
    let params: RevokeParams = body.into();
    svc.revoke_token(&mut ctx, params).await?;
    WebResponse::json(AuthRevokeRes { revoked: true })
}

// --- Google OAuth: initiate ---
#[axum::debug_handler]
pub async fn oauth_google_initiate(
    app: Extension<App>,
    body: JsonReqResult<AuthOAuthInitiateReq>,
) -> JsonResResult<WebResponse<AuthOAuthInitiateRes>> {
    let Json(body) = body?;
    let mut ctx = app.system_context()?;
    let svc = app.svc_reg.auth.clone();
    let params: OAuthInitiateParams = body.into();
    let auth_url = svc.initiate_google_oauth(&mut ctx, params).await?;
    WebResponse::json(AuthOAuthInitiateRes { auth_url })
}

// --- Google OAuth: callback ---
#[derive(Debug, Deserialize)]
pub struct GoogleOAuthCallbackQuery {
    pub code: String,
    pub state: String,
}

#[axum::debug_handler]
pub async fn oauth_google_callback(
    app: Extension<App>,
    Query(query): Query<GoogleOAuthCallbackQuery>,
) -> Result<Redirect, WebError> {
    let mut ctx = app.system_context()?;
    let svc = app.svc_reg.auth.clone();
    let params = OAuthCallbackParams {
        code: query.code,
        state: query.state,
    };
    let result = svc.process_google_callback(&mut ctx, params).await?;
    let redirect = format!(
        "{}?token={}&refresh_token={}",
        result.redirect_url, result.access_token, result.refresh_token
    );
    Ok(Redirect::to(&redirect))
}

// --- Auth Router ---
pub struct AuthRouter;

impl AuthRouter {
    /// Public auth endpoints — no authentication required.
    pub fn public_routes() -> Router {
        Router::new()
            .route("/register", post(register))
            .route("/login", post(login))
            .route("/refresh", post(refresh))
            .route("/reset-password", post(reset_password))
            .route("/update-password", post(update_password))
            .route("/confirm", post(confirm_account))
            .route("/resend-confirm", post(resend_confirm))
            .route("/oauth/google/initiate", post(oauth_google_initiate))
            .route("/oauth/google/callback", get(oauth_google_callback))
    }

    /// Protected auth endpoints — require authentication via CtxLayer.
    pub fn protected_routes() -> Router {
        Router::new().route("/revoke", post(revoke))
    }
}

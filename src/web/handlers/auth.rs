use axum::{
    Json, Router,
    extract::{Extension, Query},
    http::HeaderMap,
    response::Redirect,
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    app::App,
    core::{ctx::CoreCtx, services::token::TokenService},
    store::dbx::PgDbx,
    web::{
        dtos::auth::{
            AuthConfirmAccountReq, AuthConfirmAccountRes, AuthLoginReq, AuthLoginRes,
            AuthOAuthInitiateReq, AuthOAuthInitiateRes, AuthRefreshRes, AuthRegisterReq,
            AuthRegisterRes, AuthResendConfirmReq, AuthResendConfirmRes, AuthResetPasswordReq,
            AuthResetPasswordRes, AuthRevokeRes, AuthUpdatePasswordReq, AuthUpdatePasswordRes,
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
    let svc = app.svc_factory.auth();
    let result = svc
        .register(
            &body.email,
            body.password.as_deref().unwrap_or(""),
            body.name.as_deref(),
        )
        .await?;
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
    let svc = app.svc_factory.auth();
    let result = svc.login(&body.email, &body.password).await?;
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
    headers: HeaderMap,
) -> JsonResResult<WebResponse<AuthRefreshRes>> {
    let raw_token =
        TokenService::<PgDbx>::token_str_from_headers(&headers).ok_or(WebError::Unauthorized)?;
    let svc = app.svc_factory.auth();
    let tp = svc.refresh_token(raw_token).await?;
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
    let svc = app.svc_factory.auth();
    svc.request_password_reset(&body.email).await?;
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
    let svc = app.svc_factory.auth();
    let account_id = svc.update_password(&body.token, &body.password).await?;
    WebResponse::json(AuthUpdatePasswordRes { account_id })
}

// --- Confirm Account ---
#[axum::debug_handler]
pub async fn confirm_account(
    app: Extension<App>,
    body: JsonReqResult<AuthConfirmAccountReq>,
) -> JsonResResult<WebResponse<AuthConfirmAccountRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.auth();
    let result = svc.confirm_account(&body.token).await?;
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
    let svc = app.svc_factory.auth();
    svc.resend_confirmation(&body.email).await?;
    WebResponse::json(AuthResendConfirmRes {
        message: "if the account exists, a confirmation email has been sent".to_string(),
    })
}

// --- Revoke Token ---
#[axum::debug_handler]
pub async fn revoke(
    ctx: Extension<CoreCtx>,
    app: Extension<App>,
    headers: HeaderMap,
) -> JsonResResult<WebResponse<AuthRevokeRes>> {
    let raw_token =
        TokenService::<PgDbx>::token_str_from_headers(&headers).ok_or(WebError::Unauthorized)?;
    let svc = app.svc_factory.auth();
    svc.revoke_token(&ctx, raw_token).await?;
    WebResponse::json(AuthRevokeRes { revoked: true })
}

// --- Google OAuth: initiate ---
#[axum::debug_handler]
pub async fn oauth_google_initiate(
    app: Extension<App>,
    body: JsonReqResult<AuthOAuthInitiateReq>,
) -> JsonResResult<WebResponse<AuthOAuthInitiateRes>> {
    let Json(body) = body?;
    let svc = app.svc_factory.auth();
    let auth_url = svc.initiate_google_oauth(&body.redirect_url).await?;
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
    let svc = app.svc_factory.auth();
    let result = svc
        .process_google_callback(&query.code, &query.state)
        .await?;
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

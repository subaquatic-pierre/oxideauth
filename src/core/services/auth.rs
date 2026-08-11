use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::Duration;
use tracing::info;
use uuid::Uuid;

use crate::{
    cache::{
        entities::oauth_state::{OAuthProvider, OAuthStateCache},
        manager::CacheManager,
        traits::CacheExecutor,
    },
    config::Config,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{
                Account, AccountCreateParams, AccountKind, AccountMeta, AccountUpdateParams,
            },
            auth::RegisterParams,
            credential::{
                CredentialConfig, CredentialCreateParams, CredentialFilter, CredentialUpdateParams,
            },
            list::RequestFilterParams,
            membership::{
                MembershipCreateParams, MembershipFilter, MembershipListParams, MembershipMeta,
            },
            permission::{PermissionEngine, PermissionRule},
            role::RoleListParams,
            token::{TokenClaims, TokenType},
        },
        services::{
            account::AccountService, credential::CredentialService, membership::MembershipService,
            permission::CANONICAL_PERMISSIONS, role::RoleService, token::TokenService,
            workspace::WorkspaceService,
        },
        traits::{
            params::ValidateParams,
            service::{
                CoreModelCreateService, CoreModelListService, CoreModelService,
                CoreModelUpdateService,
            },
        },
    },
    store::{
        ctx::StoreCtx,
        entities::{
            account::AccountForUpdate,
            credential::{CredentialKind, CredentialMeta, CredentialProvider, CredentialStatus},
            membership::{MembershipScope, MembershipStatus},
        },
        manager::StoreManager,
        traits::{crud::*, dbx::DbExecutor},
    },
    utils::{
        auth::{get_google_user, request_google_token},
        crypt::{hash_password, verify_password},
        time::now_utc,
    },
};

/// Return value for `AuthService::register()` and `AuthService::login()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub account: Account,
    pub access_token: String,
    pub refresh_token: String,
}

/// Return value for `AuthService::refresh_token()` and `AuthService::issue_token_pair()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
}

/// Return value for `AuthService::confirm_account()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfirmation {
    pub account_id: Uuid,
    pub was_already_verified: bool,
}

/// Return value for `AuthService::process_google_callback()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCallbackResult {
    pub account: Account,
    pub access_token: String,
    pub refresh_token: String,
    pub redirect_url: String,
}

pub struct AuthService<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    sm: Arc<StoreManager<D>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    acc_svc: Arc<AccountService<D, C>>,
    token_svc: Arc<TokenService<D, C>>,
    credential_svc: Arc<CredentialService<D, C>>,
    membership_svc: Arc<MembershipService<D, C>>,
    role_svc: Arc<RoleService<D, C>>,
    cm: Arc<CacheManager<C>>,
    config: Config,
}

impl<D, C> AuthService<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    pub fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        acc_svc: Arc<AccountService<D, C>>,
        token_svc: Arc<TokenService<D, C>>,
        credential_svc: Arc<CredentialService<D, C>>,
        membership_svc: Arc<MembershipService<D, C>>,
        role_svc: Arc<RoleService<D, C>>,
        config: Config,
    ) -> Self {
        Self {
            sm,
            acc_svc,
            ws_svc,
            token_svc,
            credential_svc,
            membership_svc,
            role_svc,
            cm,
            config,
        }
    }

    /// Issues a fresh access + refresh token pair for a new session.
    ///
    /// Both tokens share the same session id (`sid`) and carry the current
    /// membership/account version claims. Each gets a unique `jti`.
    fn issue_token_pair(
        &self,
        account_id: Uuid,
        ws_id: Uuid,
        mem_id: Uuid,
        mem_ver: u64,
        acc_ver: u64,
        sid: Uuid,
    ) -> CoreResult<TokenPair> {
        let now = now_utc();

        let access_claims = TokenClaims::new(
            account_id,
            ws_id,
            mem_id,
            now + Duration::seconds(self.config.access_token_max_age as i64),
            TokenType::Auth,
            mem_ver,
            acc_ver,
            Some(sid),
            Some(Uuid::new_v4()),
        );

        let refresh_claims = TokenClaims::new(
            account_id,
            ws_id,
            mem_id,
            now + Duration::seconds(self.config.refresh_token_max_age as i64),
            TokenType::Refresh,
            mem_ver,
            acc_ver,
            Some(sid),
            Some(Uuid::new_v4()),
        );

        let access_token = self.token_svc.encode_token_claims(&access_claims)?;
        let refresh_token = self.token_svc.encode_token_claims(&refresh_claims)?;

        Ok(TokenPair {
            access_token,
            refresh_token,
        })
    }

    /// Registers a new account with a `Local` password credential in the
    /// specified workspace, creates a membership with the default "Workspace
    /// Viewer" role, and returns the account along with a freshly issued token pair.
    pub async fn register(
        &self,
        ctx: &mut CoreCtx,
        params: RegisterParams,
    ) -> CoreResult<AuthResult> {
        let RegisterParams {
            email,
            password,
            name,
            workspace_id,
        } = params.validate()?;

        // --- Resolve target workspace (slug takes precedence, then id) ---
        let store_ctx: StoreCtx = (&*ctx).into();
        let ws = self
            .ws_svc
            .get_workspace_by_slug_or_id(&ctx, &workspace_id)
            .await?
            .ok_or(CoreError::Auth(format!(
                "Workspace does not exist {workspace_id}"
            )))?;

        let ws_id = ws.id;

        // --- Look up the default "Workspace Viewer" role ---
        let viewer_role = self
            .role_svc
            .store()
            .get_by_name_opt(&store_ctx, "Workspace Viewer", ws.id.into())
            .await?
            .ok_or_else(|| {
                CoreError::InvalidParams(
                    "Workspace Viewer role not found — workspace may not be seeded yet".to_string(),
                )
            })?;

        // --- Check email uniqueness ---
        if self
            .acc_svc
            .store()
            .get_by_email(&store_ctx, &email)
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists(format!(
                "account with email '{}' already exists",
                email
            )));
        }

        // --- Hash password ---
        let secret = hash_password(&password)?;

        ctx.extend_perms(&[
            CANONICAL_PERMISSIONS.account.create,
            CANONICAL_PERMISSIONS.credential.create,
            CANONICAL_PERMISSIONS.membership.create,
        ])?;
        let store_ctx: StoreCtx = (&*ctx).into();

        // --- Create account (via store — AccountService::create is too heavy with validation) ---
        let default_avatar = format!("https://www.gravatar.com/avatar/{}?d=identicon", "default");
        let account_create_params = AccountCreateParams {
            email: email.clone(),
            password: String::new(), // password goes to credential, not account
            name: name.unwrap_or_else(|| "".to_string()),
            workspace_id: ws_id,
            description: None,
            avatar_url: Some(default_avatar),
            tags: None,
            meta: Some(AccountMeta {
                schema_version: "1".to_string(),
            }),
        };
        let account_row = self
            .acc_svc
            .store()
            .create(
                &store_ctx,
                account_create_params.into_store_params(AccountKind::User, true, false),
            )
            .await?;
        let acc_ver = account_row.version as u64;
        let account: Account = account_row.into();

        // --- Create credential (via service) ---
        self.credential_svc
            .create(
                ctx,
                CredentialCreateParams {
                    account_id: account.id,
                    workspace_id: ws_id,
                    kind: CredentialKind::Password,
                    provider: CredentialProvider::Local,
                    status: CredentialStatus::Active,
                    secret: Some(secret),
                    email: Some(email.clone()),
                    // TODO: get default credential from workspace config
                    config: CredentialConfig::default(),
                    provider_id: None,
                    last_used_at: None,
                    tags: vec![],
                    meta: CredentialMeta {
                        schema_version: "1".to_string(),
                    },
                },
            )
            .await?;

        // --- Create membership with Viewer role (via service) ---
        let membership = self
            .membership_svc
            .create(
                ctx,
                MembershipCreateParams {
                    account_id: account.id,
                    workspace_id: ws_id,
                    scope: MembershipScope::Workspace,
                    status: MembershipStatus::Active,
                    project_id: None,
                    role_ids: vec![viewer_role.id.into()],
                    tags: vec![],
                    meta: MembershipMeta {
                        schema_version: "1".to_string(),
                    },
                },
            )
            .await?;

        // --- Issue token pair with real workspace + membership IDs ---
        let sid = Uuid::new_v4();
        let tp = self.issue_token_pair(
            account.id,
            ws_id,
            membership.id,
            0, // mem_ver: initial version
            acc_ver,
            sid,
        )?;

        info!(
            email = %email,
            account_id = %account.id,
            workspace_id = %ws_id,
            "AUTH_REGISTER"
        );

        Ok(AuthResult {
            account,
            access_token: tp.access_token,
            refresh_token: tp.refresh_token,
        })
    }

    /// Logs an account in via email/password and returns the account along
    /// with a token pair (access + refresh).
    pub async fn login(
        &self,
        ctx: &mut CoreCtx,
        email: &str,
        password: &str,
    ) -> CoreResult<AuthResult> {
        if email.trim().is_empty() || password.is_empty() {
            return Err(CoreError::InvalidParams(
                "email and password required".to_string(),
            ));
        }

        let email = email.trim().to_lowercase();

        ctx.extend_perms(&[CANONICAL_PERMISSIONS.account.describe])?;

        // --- Find account by email ---
        let account_row = match self.acc_svc.get_by_email(&ctx, &email).await? {
            Some(row) => row,
            None => {
                info!(
                    email = %email,
                    reason = "invalid credentials",
                    "AUTH_LOGIN_FAILED"
                );
                return Err(CoreError::Auth("invalid credentials".to_string()));
            }
        };
        let acc_ver = account_row.version as u64;
        let account: Account = account_row.into();

        // --- Check account status ---
        if !account.enabled {
            info!(email = %email, reason = "account disabled", "AUTH_LOGIN_FAILED");
            return Err(CoreError::Auth("account disabled".to_string()));
        }

        // --- Find Local password credential for the account (cross-workspace query via store) ---
        let filter: CredentialFilter = json!({
            "account_id": account.id.to_string(),
            "provider": CredentialProvider::Local.to_string()
        })
        .try_into()?;

        let credentials = self
            .credential_svc
            .store()
            .list(&ctx.into(), Some(filter), None)
            .await?;
        let credential = credentials
            .into_iter()
            .find(|c| c.provider == CredentialProvider::Local && c.secret.is_some())
            .ok_or_else(|| {
                info!(
                    email = %email,
                    reason = "invalid credentials",
                    "AUTH_LOGIN_FAILED"
                );
                CoreError::Auth("invalid credentials".to_string())
            })?;

        let secret = credential.secret.as_deref().ok_or_else(|| {
            info!(
                email = %email,
                reason = "invalid credentials",
                "AUTH_LOGIN_FAILED"
            );
            CoreError::Auth("invalid credentials".to_string())
        })?;

        // --- Verify password ---
        if !verify_password(secret, password)? {
            info!(
                email = %email,
                reason = "invalid credentials",
                "AUTH_LOGIN_FAILED"
            );
            return Err(CoreError::Auth("invalid credentials".to_string()));
        }

        // --- Resolve the account's membership (for its token version) ---
        let membership_filter: MembershipFilter = json!({
            "account_id": account.id.to_string(),
            "workspace_id": credential.workspace_id.to_string()
        })
        .try_into()?;

        ctx.extend_perms(&[CANONICAL_PERMISSIONS.membership.list])?;
        let membership_list_params = MembershipListParams {
            workspace_id: credential.workspace_id.into(),
            filter: Some(RequestFilterParams {
                fields: Some(membership_filter),
                tags: None,
            }),
            options: None,
        };
        let membership_response = self
            .membership_svc
            .list(ctx, membership_list_params)
            .await?;
        let membership = membership_response.data.into_iter().next();
        let (mem_id, mem_ver) = membership
            .map(|m| (m.id, m.version as u64))
            .unwrap_or((Uuid::nil(), 0));

        // --- Issue token pair (access + refresh) ---
        let sid = Uuid::new_v4();
        let tp = self.issue_token_pair(
            account.id,
            credential.workspace_id.into(), // workspace
            mem_id,                         // membership, set up separately
            mem_ver,
            acc_ver,
            sid,
        )?;

        info!(
            email = %email,
            account_id = %account.id,
            "AUTH_LOGIN_SUCCESS"
        );

        Ok(AuthResult {
            account,
            access_token: tp.access_token,
            refresh_token: tp.refresh_token,
        })
    }

    pub async fn register_account(&self, ctx: &CoreCtx) -> CoreResult<()> {
        Ok(())
    }

    /// Revokes the given bearer token (access or refresh).
    ///
    /// Decodes the token to recover its claims, then invalidates the entire
    /// session: the session version counter is bumped and every auth-cache key
    /// belonging to the membership/account is purged. No hash computation and
    /// no database write are involved — revocation is purely version/cache
    /// based.
    pub async fn revoke_token(&self, ctx: &CoreCtx, raw_token: &str) -> CoreResult<bool> {
        // Decode the token to recover claims (sid, sub, mem, ...). A token that
        // fails signature validation or is already expired cannot be revoked.
        let claims = self.token_svc.decode_token_str(raw_token)?;

        let sid = claims.require_sid()?;
        let mem_id = claims.mem_id()?;
        let acc_id = claims.acc_id()?;

        // Authorization: only the token owner may revoke their own token.
        // TODO: workspace admin (admin permission on the token's workspace) and
        //   super admin (global permission) revocation checks.
        if ctx.account_id() != acc_id {
            return Err(CoreError::Auth(
                "unauthorized: not the token owner".to_string(),
            ));
        }

        // Purge the cached auth data for the membership + account.
        self.cm
            .invalidation
            .invalidate(mem_id, acc_id, Some(sid))
            .await?;

        // TODO(T033): Push notification trigger — notify the workspace's clients
        // that a token was revoked. Requires wiring a `ClientService` dependency
        // into `AuthService` (constructor + factory). Then call:
        //     let ws_id = Uuid::from_str(claims.ws()).unwrap_or_default();
        //     client_svc.push_to_workspace(
        //         ws_id,
        //         "token_revoked",
        //         serde_json::json!({ "account_id": acc_id, "membership_id": mem_id }),
        //         ctx, // note: needs &mut CoreCtx; revoke_token currently takes &CoreCtx
        //     ).await;

        info!(account_id = %ctx.account_id(), sid = %sid, "AUTH_TOKEN_REVOKED");

        Ok(true)
    }

    /// Rotates a refresh token: verifies it has not already been used (replay
    /// detection), consumes it, and issues a new access + refresh token pair
    /// that continues the same session.
    ///
    /// # Replay detection
    ///
    /// Each refresh token's `jti` is recorded under `oxauth:crt:{jti}` for the
    /// remainder of its lifetime. A second use of the same token means the
    /// session is compromised: the session version is bumped (invalidating
    /// every outstanding token for the session) and the cached auth data is
    /// purged.
    pub async fn refresh_token(&self, raw_token: &str) -> CoreResult<TokenPair> {
        // Decode the refresh token.
        let claims = self.token_svc.decode_token_str(raw_token)?;
        let validated = claims.validate_refresh()?;
        let sid = validated.sid;
        let jti = validated.jti;
        let acc_id = validated.account_id;
        let ws_id = validated.workspace_id;
        let mem_id = validated.membership_id;

        // --- Replay check + consume (single-use) ---
        let remaining_ttl = claims
            .exp
            .saturating_sub(now_utc().unix_timestamp() as usize);
        if remaining_ttl == 0 {
            return Err(CoreError::Auth("token expired".to_string()));
        }
        if self
            .cm
            .replay
            .check_and_consume(jti, sid, remaining_ttl as u64)
            .await?
        {
            // REPLAY DETECTED: the refresh token was already used. Compromise
            // the whole session so neither the old nor the (stolen) new tokens
            // remain valid.
            self.cm
                .invalidation
                .invalidate(mem_id, acc_id, Some(sid))
                .await?;

            return Err(CoreError::Auth(
                "session compromised, please re-authenticate".to_string(),
            ));
        }

        // --- Issue the next token pair (same session, new jtis) ---
        let now = now_utc();

        let access_claims = TokenClaims::new(
            acc_id,
            ws_id,
            mem_id,
            now + Duration::seconds(self.config.access_token_max_age as i64),
            TokenType::Auth,
            claims.mem_ver,
            claims.acc_ver,
            Some(sid),
            Some(Uuid::new_v4()),
        );

        let refresh_claims = TokenClaims::new(
            acc_id,
            ws_id,
            mem_id,
            now + Duration::seconds(self.config.refresh_token_max_age as i64),
            TokenType::Refresh,
            claims.mem_ver,
            claims.acc_ver,
            Some(sid),
            Some(Uuid::new_v4()),
        );

        let access_token = self.token_svc.encode_token_claims(&access_claims)?;
        let refresh_token = self.token_svc.encode_token_claims(&refresh_claims)?;

        info!(account_id = %acc_id, sid = %sid, "AUTH_TOKEN_REFRESHED");

        Ok(TokenPair {
            access_token,
            refresh_token,
        })
    }

    /// Requests a password reset for the given email.
    pub async fn request_password_reset(&self, ctx: &mut CoreCtx, email: &str) -> CoreResult<()> {
        let email = email.trim().to_lowercase();

        ctx.extend_perms(&[CANONICAL_PERMISSIONS.account.describe])?;
        let store = self.acc_svc.store();

        // Anti-enumeration: silently succeed if the account does not exist.
        let Some(account_row) = store.get_by_email(&ctx.into(), &email).await? else {
            return Ok(());
        };
        let account: Account = account_row.into();

        // Silently succeed for disabled accounts as well.
        if !account.enabled {
            return Ok(());
        }

        // Generate a PasswordReset token (24h TTL per spec).
        let claims = TokenClaims::new(
            account.id,
            Uuid::nil(), // workspace/membership are not needed for reset
            Uuid::nil(),
            now_utc() + Duration::hours(24),
            TokenType::PasswordReset,
            0, // mem_ver
            0, // acc_ver
            None,
            Some(Uuid::new_v4()),
        );
        let token = self.token_svc.encode_token_claims(&claims)?;

        // TODO(T061): send the reset email fire-and-forget.
        //   Once EmailService is reachable from AuthService (it requires
        //   Config + StorageService and is currently not wired into the app),
        //   spawn a tokio task after generating the token:
        //     let email_svc = self.email_svc.clone();
        //     let email = email.clone();
        //     tokio::spawn(async move {
        //         let mut ctx = tera::Context::new();
        //         ctx.insert("reset_url", &format!("{}/reset?token={token}", base_url));
        //         ctx.insert("year", "2026");
        //         let _ = email_svc
        //             .send_email(&email, "Reset your password", "emails/reset_password.html", ctx)
        //             .await;
        //     });
        //   For now only log the token.
        info!("password reset token for {email}: {token}");

        Ok(())
    }

    /// Updates an account's password using a valid `PasswordReset` token.
    ///
    /// Returns the id of the account whose password was updated.
    pub async fn update_password(
        &self,
        ctx: &mut CoreCtx,
        token: &str,
        new_password: &str,
    ) -> CoreResult<Uuid> {
        if new_password.is_empty() {
            return Err(CoreError::InvalidParams("password required".to_string()));
        }

        // Decode and validate the token.
        let claims = self.token_svc.decode_token_str(token)?;
        let account_id = claims.validate_password_reset()?;

        ctx.extend_perms(&[
            CANONICAL_PERMISSIONS.account.describe,
            CANONICAL_PERMISSIONS.credential.describe,
            CANONICAL_PERMISSIONS.credential.list,
            CANONICAL_PERMISSIONS.credential.update,
        ])?;
        let store = self.acc_svc.store();

        // Load the account and reject disabled accounts.
        let account_row = store.get(&ctx.into(), &account_id.into()).await?;
        let account: Account = account_row.into();
        if !account.enabled {
            return Err(CoreError::Auth("account disabled".to_string()));
        }

        // Hash the new password.
        let secret = hash_password(new_password)?;

        // Find the account's Local password credential (cross-workspace query via store).
        let filter: CredentialFilter = json!({
            "account_id": account.id.to_string(),
            "provider": CredentialProvider::Local.to_string()
        })
        .try_into()?;

        let credentials = self
            .credential_svc
            .store()
            .list(&ctx.into(), Some(filter), None)
            .await?;
        let credential = credentials
            .into_iter()
            .find(|c| c.provider == CredentialProvider::Local && c.secret.is_some())
            .ok_or_else(|| CoreError::Auth("invalid credentials".to_string()))?;

        // Update the stored password hash via the credential service.
        let cred_update_params = CredentialUpdateParams {
            id: credential.id.into(),
            provider_id: None,
            email: None,
            account_id: account.id,
            workspace_id: credential.workspace_id.into(),
            kind: None,
            provider: None,
            status: None,
            new_provider_id: None,
            new_email: None,
            secret: Some(secret),
            last_used_at: None,
            config: None,
            tags: None,
            meta: None,
        };
        self.credential_svc.update(ctx, cred_update_params).await?;

        info!(account_id = %account.id, "AUTH_PASSWORD_CHANGED");

        Ok(account.id)
    }

    /// Confirms an account by setting `verified = true` using a valid
    /// `AccountConfirm` token.
    ///
    /// Returns the confirmed account id and its new verified state.
    pub async fn confirm_account(
        &self,
        ctx: &mut CoreCtx,
        token: &str,
    ) -> CoreResult<AccountConfirmation> {
        // Decode and validate the token.
        let claims = self.token_svc.decode_token_str(token)?;
        let account_id = claims.validate_account_confirm()?;

        ctx.extend_perms(&[
            CANONICAL_PERMISSIONS.account.describe,
            CANONICAL_PERMISSIONS.account.update,
        ])?;
        let store = self.acc_svc.store();

        let account_row = store.get(&ctx.into(), &account_id.into()).await?;
        let account: Account = account_row.into();

        // Reject accounts that have already been verified.
        if account.verified {
            return Err(CoreError::AlreadyExists(
                "account already verified".to_string(),
            ));
        }

        // Mark the account as verified.
        let update_params = AccountUpdateParams {
            workspace_id: Uuid::nil(),
            email: None,
            id: None,
            name: None,
            description: None,
            avatar_url: None,
            version: None,
            enabled: None,
            verified: Some(true),
            tags: None,
            meta: None,
        };
        let update_data: AccountForUpdate = update_params.into();
        store
            .update(&ctx.into(), &account_id.into(), update_data)
            .await?;

        info!(account_id = %account.id, "AUTH_ACCOUNT_CONFIRMED");

        Ok(AccountConfirmation {
            account_id: account.id,
            was_already_verified: false,
        })
    }

    /// Resends an account confirmation email for the given email.
    ///
    /// Anti-enumeration: silently succeeds if the account does not exist.
    /// Errors if the account is already verified. Email delivery is stubbed for
    /// now: the token is only logged.
    pub async fn resend_confirmation(&self, ctx: &mut CoreCtx, email: &str) -> CoreResult<()> {
        let email = email.trim().to_lowercase();

        ctx.extend_perms(&[CANONICAL_PERMISSIONS.account.describe])?;
        let store = self.acc_svc.store();

        // Anti-enumeration: silently succeed if the account does not exist.
        let Some(account_row) = store.get_by_email(&ctx.into(), &email).await? else {
            return Ok(());
        };
        let account: Account = account_row.into();

        // Already verified accounts have no need for another confirmation.
        if account.verified {
            return Err(CoreError::AlreadyExists(
                "account already verified".to_string(),
            ));
        }

        // Generate a new AccountConfirm token (24h TTL per spec).
        let claims = TokenClaims::new(
            account.id,
            Uuid::nil(), // workspace/membership are not needed for confirmation
            Uuid::nil(),
            now_utc() + Duration::hours(24),
            TokenType::AccountConfirm,
            0, // mem_ver
            0, // acc_ver
            None,
            Some(Uuid::new_v4()),
        );
        let token = self.token_svc.encode_token_claims(&claims)?;

        // TODO(T061): send the confirmation email fire-and-forget.
        //   Once EmailService is reachable from AuthService (it requires
        //   Config + StorageService and is currently not wired into the app),
        //   spawn a tokio task after generating the token:
        //     let email_svc = self.email_svc.clone();
        //     let email = email.clone();
        //     let name = account.name.clone();
        //     tokio::spawn(async move {
        //         let mut ctx = tera::Context::new();
        //         ctx.insert("project_name", "OxideAuth");
        //         ctx.insert("name", &name);
        //         ctx.insert("confirm_link", &format!("{}/confirm?token={token}", base_url));
        //         ctx.insert("year", "2026");
        //         let _ = email_svc
        //             .send_email(&email, "Confirm your email", "emails/confirm_email.html", ctx)
        //             .await;
        //     });
        //   For now only log the token.
        info!("account confirmation token for {email}: {token}");

        Ok(())
    }

    pub async fn request_token(&self, ctx: &CoreCtx) -> CoreResult<()> {
        Ok(())
    }

    /// Starts a Google OAuth2 login flow.
    ///
    /// Generates a CSRF token, persists an OAuth state (containing the client
    /// `redirect_url`) in Redis for 10 minutes, and returns the Google
    /// authorization URL the client should be redirected to.
    pub async fn initiate_google_oauth(
        &self,
        ctx: &mut CoreCtx,
        redirect_url: &str,
    ) -> CoreResult<String> {
        // Generate the CSRF token used as the OAuth `state` parameter.
        let csrf_token = Uuid::new_v4().to_string();

        // Persist the OAuth state in Redis (10 minute TTL).
        let oauth_entity = OAuthStateCache {
            csrf_token: csrf_token.clone(),
            redirect_url: redirect_url.to_string(),
            created_at: now_utc().unix_timestamp(),
            provider: OAuthProvider::Google,
        };
        self.cm.oauth_state.write(&oauth_entity, 600).await?;

        // Build the Google authorization URL.
        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid+email+profile&state={}",
            self.config.google_oauth_client_id, self.config.google_oauth_redirect_url, csrf_token,
        );

        Ok(auth_url)
    }

    /// Handles the Google OAuth2 callback.
    ///
    /// Validates the `state` parameter against the stored CSRF token, exchanges
    /// the authorization `code` for access/id tokens, fetches the Google user
    /// profile, and resolves the matching account (creating one on first
    /// sign-in together with a Google `OAuth` credential). Returns the account,
    /// a freshly issued auth JWT, and the client redirect URL recovered from the
    /// stored OAuth state.
    pub async fn process_google_callback(
        &self,
        ctx: &mut CoreCtx,
        code: &str,
        state: &str,
    ) -> CoreResult<OAuthCallbackResult> {
        // 1. Validate the CSRF state persisted during initiation.
        let oauth_entity = self
            .cm
            .oauth_state
            .fetch_and_consume(state)
            .await
            .map_err(|_| CoreError::Auth("invalid oauth state".to_string()))?;

        // 2. Exchange the authorization code for tokens.
        let token_response = request_google_token(code, &self.config).await?;

        // 3. Fetch the Google user profile.
        let google_user =
            get_google_user(&token_response.access_token, &token_response.id_token).await?;

        ctx.extend_perms(&[
            CANONICAL_PERMISSIONS.account.describe,
            CANONICAL_PERMISSIONS.account.create,
            CANONICAL_PERMISSIONS.credential.create,
            CANONICAL_PERMISSIONS.credential.describe,
        ])?;
        let store = self.acc_svc.store();

        // 4. Resolve an existing account by email, or create a new one.
        let store_ctx = StoreCtx::from(&*ctx);
        let account =
            if let Some(existing) = store.get_by_email(&store_ctx, &google_user.email).await? {
                let account: Account = existing.into();
                if !account.enabled {
                    return Err(CoreError::Auth("account disabled".to_string()));
                }
                account
            } else {
                // 5. Create the account from the Google profile.
                let account_create_params = AccountCreateParams {
                    email: google_user.email.clone(),
                    password: String::new(),
                    name: google_user.name,
                    workspace_id: store_ctx.ws_id,
                    description: None,
                    avatar_url: google_user.picture.clone(),
                    tags: None,
                    meta: Some(AccountMeta {
                        schema_version: "1".to_string(),
                    }),
                };
                let account: Account = self
                    .acc_svc
                    .store()
                    .create(
                        &ctx.into(),
                        account_create_params.into_store_params(
                            AccountKind::User,
                            true,
                            google_user.verified_email,
                        ),
                    )
                    .await?
                    .into();

                // 6. Link the Google OAuth credential to the new account.
                self.credential_svc
                    .create(
                        ctx,
                        CredentialCreateParams {
                            account_id: account.id,
                            workspace_id: store_ctx.ws_id,
                            kind: CredentialKind::OAuth,
                            provider: CredentialProvider::Google,
                            status: CredentialStatus::Active,
                            provider_id: Some(google_user.id),
                            email: Some(google_user.email),
                            config: CredentialConfig::default(),
                            secret: None,
                            last_used_at: Some(now_utc()),
                            tags: vec![],
                            meta: CredentialMeta {
                                schema_version: "1".to_string(),
                            },
                        },
                    )
                    .await?;

                account
            };

        // 7. Issue a token pair (access + refresh) for the session.
        let account_row = self
            .acc_svc
            .store()
            .get(&ctx.into(), &account.id.into())
            .await?;
        let acc_ver = account_row.version as u64;

        let sid = Uuid::new_v4();
        let tp = self.issue_token_pair(
            account.id,
            Uuid::nil(), // workspace, set up separately after sign-in
            Uuid::nil(), // membership, set up separately after sign-in
            0,           // mem_ver: no membership on first sign-in
            acc_ver,
            sid,
        )?;

        info!(
            email = %account.email,
            account_id = %account.id,
            provider = "google",
            "AUTH_LOGIN_SUCCESS"
        );

        Ok(OAuthCallbackResult {
            account,
            access_token: tp.access_token,
            refresh_token: tp.refresh_token,
            redirect_url: oauth_entity.redirect_url,
        })
    }
}

pub struct AuthValidator<'a> {
    ctx: &'a CoreCtx,
}

impl<'a> AuthValidator<'a> {
    pub fn new(ctx: &'a CoreCtx) -> Self {
        Self { ctx }
    }

    pub fn validate_perms<'b>(granted: &PermissionEngine, required: &[&str]) -> CoreResult<()> {
        let required = PermissionRule::perms_from_str_slice(required)?;
        let all_required_match_granted = granted.has_subset(&required);
        if (all_required_match_granted) {
            Ok(())
        } else {
            Err(CoreError::Auth("invalid permissions".to_string()))
        }
    }

    pub fn validate_ctx_perms<'b>(&self, required: &[&str]) -> CoreResult<()> {
        // info!("CTX in validate_ctx_perms: {:#?}", self.ctx);
        let required = PermissionRule::perms_from_str_slice(required)?;
        let granted = self.ctx.permission_checker();

        let all_required_match_granted = granted.has_subset(&required);
        if (all_required_match_granted) {
            Ok(())
        } else {
            Err(CoreError::Auth(format!(
                "invalid permissions, required premissions: {}",
                required
                    .iter()
                    .map(|el| el.to_string())
                    .collect::<Vec<String>>()
                    .join(",")
            )))
        }
    }

    /// Validates the requested workspace ID against the user's operational context.
    ///
    /// This function enforces the separation of tenancy by ensuring that a user
    /// operating within a scoped context (i.e., not a global/root user) can only
    /// query or mutate data within their assigned workspace.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The current operational context (`CoreCtx`), which holds the user's
    ///          authentication and assigned workspace scope.
    /// * `requested_workspace_id`: The optional workspace ID provided by the client
    ///                             (e.g., in a query filter or mutation DTO).
    ///
    /// # Behavior
    ///
    /// 1. **Global Context (Admin/Root):** If `ctx.is_system_workspace()` is true,
    ///    validation passes immediately, and the `StoreCtx` is returned.
    ///
    /// 2. **Scoped Context (Tenant User):**
    ///    * **Required:** If `requested_workspace_id` is `None`, an error is returned.
    ///    * **Authorization:** The provided `requested_workspace_id` must exactly match
    ///      the workspace ID stored in the `ctx.workspace_id()`.
    ///
    /// # Returns
    ///
    /// A `CoreResult<StoreCtx>` containing:
    ///
    /// * `Ok(StoreCtx)`: If validation succeeds a StoreCtx is created from CoreCtx.
    /// * `Err(CoreError::Auth)`: If the user is scoped and fails the validation checks.
    pub fn scope_store_workspace(
        &self,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<StoreCtx> {
        let ctx = self.ctx;
        let mut store_ctx: StoreCtx = ctx.into();
        // set workspace context
        if let Some(workspace_id) = self.validate_workspace(requested_workspace_id)? {
            store_ctx.set_workspace_scope(requested_workspace_id);
        }
        Ok(store_ctx)
    }

    /// Validates the requested workspace ID against the user's operational context.
    ///
    /// This function enforces the separation of tenancy by ensuring that a
    /// non-root user always has a concrete workspace ID to operate on.
    ///
    /// # Behavior
    ///
    /// 1. **Global Context (Admin/Root):** If `ctx.is_system_workspace()` is true,
    ///    validation passes immediately, and the `requested_workspace_id` is returned
    ///    as is (it may be `None`).
    ///
    /// 2. **Scoped Context (Tenant User):**
    ///    * **Required:** If `requested_workspace_id` is `None`, an error is returned.
    ///      (Should not happen in practice — the middleware always resolves a
    ///      concrete workspace before the service layer.)
    ///
    /// # Returns
    ///
    /// A `CoreResult<Option<Uuid>>` containing:
    ///
    /// * `Ok(Some(Uuid))`: If validation succeeds (either global or scoped with ID).
    /// * `Ok(None)`: Only if in a global context and no ID was requested.
    /// * `Err(CoreError::Auth)`: If the user is scoped and no ID was provided.
    pub fn validate_workspace(
        &self,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<Option<Uuid>> {
        let is_global_context = self.ctx.is_system_workspace()?;

        if is_global_context {
            // Case 1: Global context (admin/root).
            return Ok(requested_workspace_id);
        }

        // Case 2: Scoped context — must have a concrete workspace.
        match requested_workspace_id {
            Some(id) => {
                if self.ctx.auth_cache.auth_scope.workspace_id != id {
                    return Err(CoreError::Auth("unauthorized workspace".to_string()));
                }
                Ok(Some(id))
            }
            None => Err(CoreError::Auth("workspace_id required".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::dev::init::init_test;

    use super::*;

    fn setup_checker() -> CoreResult<PermissionEngine> {
        PermissionEngine::from_str_slice(&["project:read", "project:create", "account:*", "*:read"])
    }

    #[tokio::test]
    #[serial]
    async fn test_validate_perms() -> CoreResult<()> {
        let app = init_test().await;
        let granted = setup_checker()?;

        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&[CANONICAL_PERMISSIONS.account.create])?;
        let auth = AuthValidator::new(&ctx);

        let success = auth.validate_ctx_perms(&[CANONICAL_PERMISSIONS.account.create]);

        assert!(
            matches!(success, Ok(())),
            "should be success on validate context"
        );
        Ok(())
    }
}

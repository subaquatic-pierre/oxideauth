use std::{str::FromStr, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::Duration;
use tracing::info;
use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    config::Config,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::Account,
            permission::{PermissionCheck, PermissionChecker},
            token::{TokenClaims, TokenType},
        },
        services::{account::AccountService, token::TokenService},
        traits::service::CoreModelService,
    },
    store::{
        ctx::StoreCtx,
        entities::{
            account::{AccountForCreate, AccountForUpdate, AccountMeta},
            credential::{
                CredentialFilter, CredentialForCreate, CredentialForUpdate, CredentialKind,
                CredentialMeta, CredentialProvider, CredentialStatus,
            },
            membership::MembershipFilter,
        },
        manager::StoreManager,
        traits::{crud::{Create, Get, List, Update}, dbx::DbExecutor},
    },
    utils::{
        auth::{get_google_user, request_google_token},
        crypt::{hash_password, verify_password},
        time::now_utc,
    },
};

/// Maximum number of failed login attempts allowed per email within a window.
const MAX_LOGIN_ATTEMPTS: u32 = 5;
/// Duration (in seconds) of the login rate limiting window.
const LOGIN_WINDOW_SECS: u64 = 300;

/// Small state object persisted in Redis to track rate limiting counters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitState {
    pub count: u32,
    pub window_start: i64,
}

/// OAuth state persisted in Redis while a Google OAuth handshake is in flight.
///
/// Keyed by `oauth:state:{csrf_token}`, it lets the callback handler validate
/// the `state` parameter (CSRF protection) and recover the client redirect URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleOAuthStateCache {
    pub redirect_url: String,
    pub created_at: i64,
}

pub struct AuthService<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    sm: Arc<StoreManager<D>>,
    acc_svc: AccountService<D>,
    token_svc: TokenService<D, C>,
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
        acc_svc: AccountService<D>,
        token_svc: TokenService<D, C>,
        cm: Arc<CacheManager<C>>,
        config: Config,
    ) -> Self {
        Self {
            sm,
            acc_svc,
            token_svc,
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
    ) -> CoreResult<(String, String)> {
        let now = now_utc();

        let access_claims = TokenClaims::new(
            account_id,
            ws_id,
            mem_id,
            now + Duration::seconds(self.config.access_token_max_age as i64),
            TokenType::Auth,
            mem_ver,
            acc_ver,
            0, // sid_ver
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
            0, // sid_ver
            Some(sid),
            Some(Uuid::new_v4()),
        );

        let access_token = self.token_svc.encode_token_claims(&access_claims)?;
        let refresh_token = self.token_svc.encode_token_claims(&refresh_claims)?;

        Ok((access_token, refresh_token))
    }

    /// Registers a new account with a `Local` password credential and returns
    /// the created account along with a freshly issued token pair (access +
    /// refresh).
    pub async fn register(
        &self,
        email: &str,
        password: &str,
        name: Option<&str>,
    ) -> CoreResult<(Account, String, String)> {
        // --- Validate inputs ---
        if password.is_empty() {
            return Err(CoreError::InvalidParams(
                "password required".to_string(),
            ));
        }

        let email = email.trim().to_lowercase();
        if email.is_empty() {
            return Err(CoreError::InvalidParams("email required".to_string()));
        }

        let store = self.acc_svc.store();
        let store_ctx = StoreCtx::new_root();

        // --- Check email uniqueness ---
        if store.get_by_email(&store_ctx, &email).await?.is_some() {
            return Err(CoreError::AlreadyExists(format!(
                "account with email '{}' already exists",
                email
            )));
        }

        // --- Hash password ---
        let password_hash = hash_password(password)?;

        // --- Create account ---
        let default_avatar =
            format!("https://www.gravatar.com/avatar/{}?d=identicon", "default");

        let for_create = AccountForCreate {
            email: email.clone(),
            name: name.unwrap_or("").to_string(),
            description: None,
            avatar_url: Some(default_avatar),
            enabled: true,
            verified: false,
            tags: vec![],
            meta: AccountMeta {
                schema_version: "1".to_string(),
            },
        };

        let account_row = store.create(&store_ctx, for_create).await?;
        let acc_ver = account_row.token_version as u64;
        let account: Account = account_row.into();

        // --- Create Local password credential ---
        let credential_for_create = CredentialForCreate {
            kind: CredentialKind::Password,
            provider: CredentialProvider::Local,
            status: CredentialStatus::Active,
            account_id: account.id,
            workspace_id: store_ctx.ws_id,
            provider_id: None,
            email: Some(email),
            secret: Some(password_hash),
            last_used_at: None,
            tags: vec![],
            meta: CredentialMeta {
                schema_version: "1".to_string(),
            },
        };
        self.sm
            .credential
            .create(&store_ctx, credential_for_create)
            .await?;

        // --- Issue token pair (access + refresh) ---
        let sid = Uuid::new_v4();
        let chx = self.cm.executor();
        chx.set_string(
            &format!("oxauth:sv:{}", sid),
            "0",
            Some(self.config.refresh_token_max_age),
        )
        .await?;

        let (access_token, refresh_token) = self.issue_token_pair(
            account.id,
            Uuid::nil(), // workspace set up separately after registration
            Uuid::nil(), // membership set up separately after registration
            0,           // mem_ver: no membership yet
            acc_ver,
            sid,
        )?;

        // TODO(T061): send the welcome/confirmation email fire-and-forget.
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
        //         ctx.insert("confirm_link", &format!("{}/confirm?token={access_token}", base_url));
        //         ctx.insert("year", "2026");
        //         let _ = email_svc
        //             .send_email(&email, "Confirm your email", "emails/confirm_email.html", ctx)
        //             .await;
        //     });
        //   For now the confirmation token is only issued; no email is sent.

        info!(
            email = %account.email,
            account_id = %account.id,
            "AUTH_REGISTER"
        );

        Ok((account, access_token, refresh_token))
    }

    /// Logs an account in via email/password and returns the account along
    /// with a token pair (access + refresh). Failed attempts are rate limited
    /// (Redis-backed).
    pub async fn login(&self, email: &str, password: &str) -> CoreResult<(Account, String, String)> {
        if email.trim().is_empty() || password.is_empty() {
            return Err(CoreError::InvalidParams(
                "email and password required".to_string(),
            ));
        }

        let email = email.trim().to_lowercase();

        // --- Rate limiting (Redis-backed) ---
        let rl_key = format!("login:{}", email);
        if let Err(e) = self
            .check_rate_limit(&rl_key, MAX_LOGIN_ATTEMPTS, LOGIN_WINDOW_SECS)
            .await
        {
            info!(email = %email, reason = "rate limited", "AUTH_LOGIN_FAILED");
            return Err(e);
        }

        let store_ctx = StoreCtx::new_root();

        // --- Find account by email ---
        let store = self.acc_svc.store();
        let account_row = match store.get_by_email(&store_ctx, &email).await? {
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
        let acc_ver = account_row.token_version as u64;
        let account: Account = account_row.into();

        // --- Check account status ---
        if !account.enabled {
            info!(email = %email, reason = "account disabled", "AUTH_LOGIN_FAILED");
            return Err(CoreError::Auth("account disabled".to_string()));
        }

        // --- Find Local password credential for the account ---
        let filter: CredentialFilter = json!({
            "account_id": account.id.to_string(),
            "provider": CredentialProvider::Local.to_string()
        })
        .try_into()?;

        let credentials = self
            .sm
            .credential
            .list(&store_ctx, Some(filter), None)
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

        let password_hash = credential
            .secret
            .as_deref()
            .ok_or_else(|| {
                info!(
                    email = %email,
                    reason = "invalid credentials",
                    "AUTH_LOGIN_FAILED"
                );
                CoreError::Auth("invalid credentials".to_string())
            })?;

        // --- Verify password ---
        if !verify_password(password_hash, password)? {
            info!(
                email = %email,
                reason = "invalid credentials",
                "AUTH_LOGIN_FAILED"
            );
            return Err(CoreError::Auth("invalid credentials".to_string()));
        }

        // --- Clear rate limit on success ---
        self.reset_rate_limit(&rl_key).await?;

        // --- Resolve the account's membership (for its token version) ---
        let membership_filter: MembershipFilter = json!({
            "account_id": account.id.to_string(),
        })
        .try_into()?;
        let memberships = self
            .sm
            .membership
            .list(&store_ctx, Some(membership_filter), None)
            .await?;
        let membership = memberships.into_iter().next();
        let mem_ver = membership.map(|m| m.token_version as u64).unwrap_or(0);

        // --- Issue token pair (access + refresh) ---
        let sid = Uuid::new_v4();
        let chx = self.cm.executor();
        chx.set_string(
            &format!("oxauth:sv:{}", sid),
            "0",
            Some(self.config.refresh_token_max_age),
        )
        .await?;

        let (access_token, refresh_token) = self.issue_token_pair(
            account.id,
            Uuid::nil(), // workspace, set up separately
            Uuid::nil(), // membership, set up separately
            mem_ver,
            acc_ver,
            sid,
        )?;

        info!(
            email = %email,
            account_id = %account.id,
            "AUTH_LOGIN_SUCCESS"
        );

        Ok((account, access_token, refresh_token))
    }

    /// Enforces a sliding window rate limit for the given key.
    ///
    /// Allows `max_attempts` within a window of `window_secs` seconds. Once the
    /// limit is exceeded a `CoreError::Auth` is returned. State is persisted in
    /// Redis through the `CacheManager` executor.
    pub async fn check_rate_limit(
        &self,
        key: &str,
        max_attempts: u32,
        window_secs: u64,
    ) -> CoreResult<()> {
        let chx = self.cm.executor();
        let cache_key = format!("oxideauth:rate_limit:{}", key);
        let now = now_utc().unix_timestamp();

        let mut state = chx
            .get::<RateLimitState>(&cache_key, None)
            .await?
            .unwrap_or(RateLimitState {
                count: 0,
                window_start: now,
            });

        // Reset the window if it has elapsed.
        if now - state.window_start >= window_secs as i64 {
            state = RateLimitState {
                count: 0,
                window_start: now,
            };
        }

        if state.count >= max_attempts {
            return Err(CoreError::Auth(
                "too many attempts, try again later".to_string(),
            ));
        }

        state.count += 1;
        chx.set(&cache_key, None, &state, Some(window_secs)).await?;

        Ok(())
    }

    /// Clears the rate limit counter for the given key (called on success).
    pub async fn reset_rate_limit(&self, key: &str) -> CoreResult<()> {
        let chx = self.cm.executor();
        let cache_key = format!("oxideauth:rate_limit:{}", key);
        chx.del::<RateLimitState>(&cache_key, None).await?;
        Ok(())
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

        let sid = claims
            .sid()
            .ok_or_else(|| CoreError::Auth("invalid token".to_string()))?;
        let mem_id = Uuid::from_str(claims.mem())
            .map_err(|_| CoreError::Auth("invalid token".to_string()))?;
        let acc_id = Uuid::from_str(claims.sub())
            .map_err(|_| CoreError::Auth("invalid token".to_string()))?;

        // Authorization: only the token owner may revoke their own token.
        // TODO: workspace admin (admin permission on the token's workspace) and
        //   super admin (global permission) revocation checks.
        if ctx.account_id() != acc_id {
            return Err(CoreError::Auth(
                "unauthorized: not the token owner".to_string(),
            ));
        }

        // Invalidate the session: bump the session version so any outstanding
        // access/refresh token for this session fails the sid_ver check.
        let chx = self.cm.executor();
        let sv_key = format!("oxauth:sv:{}", sid);
        chx.incr(&sv_key).await?;

        // Purge the cached auth data for the membership + account.
        chx.del_key(&format!("oxauth:tv:{}", mem_id)).await?;
        chx.del_key(&format!("oxauth:av:{}", acc_id)).await?;
        chx.del_key(&format!("oxauth:sv:{}", sid)).await?;
        chx.del_key(&format!("oxauth:ma:{}", mem_id)).await?;
        chx.del_key(&format!("oxauth:ae:{}", acc_id)).await?;
        chx.del_key(&format!("oxauth:as:{}", mem_id)).await?;

        info!(account_id = %ctx.account_id(), sid = %sid, "AUTH_TOKEN_REVOKED");

        Ok(true)
    }

    /// Blacklists a token by its SHA-256 hash (hex string), bypassing the need
    /// for the raw token itself.
    ///
    /// Requires the `token:revokeAny` permission. The hash is written to the
    /// Redis blacklist with a long TTL (90 days). `reason` is logged for
    /// auditing. No database write is performed (the token blacklist is
    /// cache-only in this version-based revocation scheme).
    pub async fn blacklist_token(
        &self,
        ctx: &CoreCtx,
        token_hash: &str,
        reason: Option<&str>,
    ) -> CoreResult<bool> {
        // Validate admin permissions.
        AuthValidator::new(ctx).validate_ctx_perms(&["token:revokeAny"])?;

        // Validate the hex-encoded SHA-256 hash.
        hex::decode(token_hash)
            .map_err(|_| CoreError::InvalidParams("invalid token hash".to_string()))?;

        // Persist the blacklist entry in Redis with a long TTL.
        let long_ttl = 90 * 24 * 60 * 60; // 90 days
        let chx = self.cm.executor();
        let cache_key = format!("blacklist:{}", token_hash);
        let value = json!("1");
        chx.set(&cache_key, None, &value, Some(long_ttl)).await?;

        info!(
            account_id = %ctx.account_id(),
            reason = %reason.unwrap_or("admin_blacklisted"),
            "AUTH_TOKEN_BLACKLISTED"
        );

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
    pub async fn refresh_token(&self, raw_token: &str) -> CoreResult<(String, String)> {
        // Decode the refresh token.
        let claims = self.token_svc.decode_token_str(raw_token)?;
        if claims.token_type() != TokenType::Refresh {
            return Err(CoreError::Auth("invalid token type".to_string()));
        }

        let sid = claims
            .sid()
            .ok_or_else(|| CoreError::Auth("invalid refresh token".to_string()))?;
        let jti = claims
            .jti()
            .ok_or_else(|| CoreError::Auth("invalid refresh token".to_string()))?;

        let mem_id = Uuid::from_str(claims.mem())
            .map_err(|_| CoreError::Auth("invalid token".to_string()))?;
        let acc_id = Uuid::from_str(claims.sub())
            .map_err(|_| CoreError::Auth("invalid token".to_string()))?;
        let ws_id = Uuid::from_str(claims.ws())
            .map_err(|_| CoreError::Auth("invalid token".to_string()))?;

        let chx = self.cm.executor();
        let consumed_key = format!("oxauth:crt:{}", jti);

        // --- Replay check ---
        if let Some(_consumed_sid) = chx.get::<String>(&consumed_key, None).await? {
            // REPLAY DETECTED: the refresh token was already used. Compromise
            // the whole session so neither the old nor the (stolen) new tokens
            // remain valid.
            let sv_key = format!("oxauth:sv:{}", sid);
            chx.incr(&sv_key).await?;

            chx.del_key(&format!("oxauth:tv:{}", mem_id)).await?;
            chx.del_key(&format!("oxauth:av:{}", acc_id)).await?;
            chx.del_key(&format!("oxauth:sv:{}", sid)).await?;
            chx.del_key(&format!("oxauth:ma:{}", mem_id)).await?;
            chx.del_key(&format!("oxauth:ae:{}", acc_id)).await?;
            chx.del_key(&format!("oxauth:as:{}", mem_id)).await?;

            return Err(CoreError::Auth(
                "session compromised, please re-authenticate".to_string(),
            ));
        }

        // --- Consume this refresh token (single-use) ---
        let remaining_ttl = claims.exp.saturating_sub(now_utc().unix_timestamp() as usize);
        if remaining_ttl > 0 {
            chx.set(&consumed_key, None, &sid.to_string(), Some(remaining_ttl as u64))
                .await?;
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
            claims.sid_ver,
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
            claims.sid_ver,
            Some(sid),
            Some(Uuid::new_v4()),
        );

        let access_token = self.token_svc.encode_token_claims(&access_claims)?;
        let refresh_token = self.token_svc.encode_token_claims(&refresh_claims)?;

        info!(account_id = %acc_id, sid = %sid, "AUTH_TOKEN_REFRESHED");

        Ok((access_token, refresh_token))
    }

    /// Requests a password reset for the given email.
    ///
    /// Anti-enumeration: always returns `Ok(())`, whether or not the account
    /// exists or is disabled. When the account exists and is enabled a
    /// `PasswordReset` token is generated (24h TTL per spec).
    ///
    /// Email delivery is stubbed for now: the token is only logged.
    pub async fn request_password_reset(&self, email: &str) -> CoreResult<()> {
        let email = email.trim().to_lowercase();

        let store_ctx = StoreCtx::new_root();
        let store = self.acc_svc.store();

        // Anti-enumeration: silently succeed if the account does not exist.
        let Some(account_row) = store.get_by_email(&store_ctx, &email).await? else {
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
            0, // sid_ver
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
    pub async fn update_password(&self, token: &str, new_password: &str) -> CoreResult<Uuid> {
        if new_password.is_empty() {
            return Err(CoreError::InvalidParams("password required".to_string()));
        }

        // Decode and validate the token.
        let claims = self.token_svc.decode_token_str(token)?;
        if claims.token_type() != TokenType::PasswordReset {
            return Err(CoreError::Auth("invalid token type".to_string()));
        }
        if claims.is_expired() {
            return Err(CoreError::Auth("token expired".to_string()));
        }

        // Extract account id from the token subject.
        let account_id = Uuid::from_str(claims.sub())
            .map_err(|_| CoreError::InvalidParams("invalid token".to_string()))?;

        let store_ctx = StoreCtx::new_root();
        let store = self.acc_svc.store();

        // Load the account and reject disabled accounts.
        let account_row = store.get(&store_ctx, &account_id.into()).await?;
        let account: Account = account_row.into();
        if !account.enabled {
            return Err(CoreError::Auth("account disabled".to_string()));
        }

        // Hash the new password.
        let password_hash = hash_password(new_password)?;

        // Find the account's Local password credential.
        let filter: CredentialFilter = json!({
            "account_id": account.id.to_string(),
            "provider": CredentialProvider::Local.to_string()
        })
        .try_into()?;

        let credentials = self
            .sm
            .credential
            .list(&store_ctx, Some(filter), None)
            .await?;
        let credential = credentials
            .into_iter()
            .find(|c| c.provider == CredentialProvider::Local && c.secret.is_some())
            .ok_or_else(|| CoreError::Auth("invalid credentials".to_string()))?;

        // Update the stored password hash.
        let update_data = CredentialForUpdate {
            kind: None,
            provider: None,
            status: None,
            provider_id: None,
            email: None,
            secret: Some(password_hash),
            last_used_at: None,
            tags: None,
            meta: None,
        };
        self.sm
            .credential
            .update(&store_ctx, &credential.id, update_data)
            .await?;

        info!(account_id = %account.id, "AUTH_PASSWORD_CHANGED");

        Ok(account.id)
    }

    /// Confirms an account by setting `verified = true` using a valid
    /// `AccountConfirm` token.
    ///
    /// Returns the confirmed account id and its new verified state.
    pub async fn confirm_account(&self, token: &str) -> CoreResult<(Uuid, bool)> {
        // Decode and validate the token.
        let claims = self.token_svc.decode_token_str(token)?;
        if claims.token_type() != TokenType::AccountConfirm {
            return Err(CoreError::Auth("invalid token type".to_string()));
        }
        if claims.is_expired() {
            return Err(CoreError::Auth("token expired".to_string()));
        }

        // Extract account id from the token subject.
        let account_id = Uuid::from_str(claims.sub())
            .map_err(|_| CoreError::InvalidParams("invalid token".to_string()))?;

        let store_ctx = StoreCtx::new_root();
        let store = self.acc_svc.store();

        let account_row = store.get(&store_ctx, &account_id.into()).await?;
        let account: Account = account_row.into();

        // Reject accounts that have already been verified.
        if account.verified {
            return Err(CoreError::AlreadyExists(
                "account already verified".to_string(),
            ));
        }

        // Mark the account as verified.
        let update_data = AccountForUpdate {
            email: None,
            name: None,
            description: None,
            avatar_url: None,
            enabled: None,
            verified: Some(true),
            tags: None,
            meta: None,
            token_version: None,
        };
        store
            .update(&store_ctx, &account_id.into(), update_data)
            .await?;

        info!(account_id = %account.id, "AUTH_ACCOUNT_CONFIRMED");

        Ok((account.id, true))
    }

    /// Resends an account confirmation email for the given email.
    ///
    /// Anti-enumeration: silently succeeds if the account does not exist.
    /// Errors if the account is already verified. Email delivery is stubbed for
    /// now: the token is only logged.
    pub async fn resend_confirmation(&self, email: &str) -> CoreResult<()> {
        let email = email.trim().to_lowercase();

        let store_ctx = StoreCtx::new_root();
        let store = self.acc_svc.store();

        // Anti-enumeration: silently succeed if the account does not exist.
        let Some(account_row) = store.get_by_email(&store_ctx, &email).await? else {
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
            0, // sid_ver
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
    pub async fn initiate_google_oauth(&self, redirect_url: &str) -> CoreResult<String> {
        // Generate the CSRF token used as the OAuth `state` parameter.
        let csrf_token = Uuid::new_v4().to_string();

        // Persist the OAuth state in Redis (10 minute TTL).
        let state = GoogleOAuthStateCache {
            redirect_url: redirect_url.to_string(),
            created_at: now_utc().unix_timestamp(),
        };
        let chx = self.cm.executor();
        let cache_key = format!("oauth:state:{}", csrf_token);
        chx.set(&cache_key, None, &state, Some(600)).await?;

        // Build the Google authorization URL.
        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid+email+profile&state={}",
            self.config.google_oauth_client_id,
            self.config.google_oauth_redirect_url,
            csrf_token,
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
        code: &str,
        state: &str,
    ) -> CoreResult<(Account, String, String, String)> {
        // 1. Validate the CSRF state persisted during initiation.
        let chx = self.cm.executor();
        let cache_key = format!("oauth:state:{}", state);
        let oauth_state = chx
            .get::<GoogleOAuthStateCache>(&cache_key, None)
            .await?
            .ok_or_else(|| CoreError::Auth("invalid oauth state".to_string()))?;

        // The state is single-use: consume it after validation.
        chx.del::<serde_json::Value>(&cache_key, None).await?;

        // 2. Exchange the authorization code for tokens.
        let token_response = request_google_token(code, &self.config).await?;

        // 3. Fetch the Google user profile.
        let google_user = get_google_user(&token_response.access_token, &token_response.id_token)
            .await?;

        let store_ctx = StoreCtx::new_root();
        let store = self.acc_svc.store();

        // 4. Resolve an existing account by email, or create a new one.
        let account = if let Some(existing) = store
            .get_by_email(&store_ctx, &google_user.email)
            .await?
        {
            let account: Account = existing.into();
            if !account.enabled {
                return Err(CoreError::Auth("account disabled".to_string()));
            }
            account
        } else {
            // 5. Create the account from the Google profile.
            let account_for_create = AccountForCreate {
                email: google_user.email.clone(),
                name: google_user.name,
                description: None,
                avatar_url: google_user.picture.clone(),
                enabled: true,
                verified: google_user.verified_email,
                tags: vec![],
                meta: AccountMeta {
                    schema_version: "1".to_string(),
                },
            };
            let account: Account = store.create(&store_ctx, account_for_create).await?.into();

            // 6. Link the Google OAuth credential to the new account.
            let credential_for_create = CredentialForCreate {
                kind: CredentialKind::OAuth,
                provider: CredentialProvider::Google,
                status: CredentialStatus::Active,
                account_id: account.id,
                workspace_id: store_ctx.ws_id,
                provider_id: Some(google_user.id),
                email: Some(google_user.email),
                secret: None,
                last_used_at: Some(now_utc()),
                tags: vec![],
                meta: CredentialMeta {
                    schema_version: "1".to_string(),
                },
            };
            self.sm
                .credential
                .create(&store_ctx, credential_for_create)
                .await?;

            account
        };

        // 7. Issue a token pair (access + refresh) for the session.
        let store_ctx = StoreCtx::new_root();
        let acc_ver = self
            .sm
            .account
            .get(&store_ctx, &account.id.into())
            .await?
            .token_version as u64;

        let sid = Uuid::new_v4();
        let chx = self.cm.executor();
        chx.set_string(
            &format!("oxauth:sv:{}", sid),
            "0",
            Some(self.config.refresh_token_max_age),
        )
        .await?;

        let (access_token, refresh_token) = self.issue_token_pair(
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

        Ok((account, access_token, refresh_token, oauth_state.redirect_url))
    }
}

pub struct AuthValidator<'a> {
    ctx: &'a CoreCtx,
}

impl<'a> AuthValidator<'a> {
    pub fn new(ctx: &'a CoreCtx) -> Self {
        Self { ctx }
    }

    pub fn validate_perms<'b>(granted: &PermissionChecker, required: &[&str]) -> CoreResult<()> {
        let required = PermissionCheck::perms_from_str_slice(required)?;
        let all_required_match_granted = granted.has_subset(&required);
        if (all_required_match_granted) {
            Ok(())
        } else {
            Err(CoreError::Auth("invalid permissions".to_string()))
        }
    }

    pub fn validate_ctx_perms<'b>(&self, required: &[&str]) -> CoreResult<()> {
        let required = PermissionCheck::perms_from_str_slice(required)?;
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
    /// 1. **Global Context (Admin/Root):** If `ctx.is_global_workspace()` is true,
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
    /// 1. **Global Context (Admin/Root):** If `ctx.is_global_workspace()` is true,
    ///    validation passes immediately, and the `requested_workspace_id` is returned
    ///    as is (it may be `None`).
    ///
    /// 2. **Scoped Context (Tenant User):**
    ///    * **Required:** If `requested_workspace_id` is `None`, an error is returned.
    ///    * **Authorization:** The provided `requested_workspace_id` must exactly match
    ///      the workspace ID stored in the `ctx.workspace_id()`.
    ///
    /// # Returns
    ///
    /// A `CoreResult<Option<Uuid>>` containing:
    ///
    /// * `Ok(Some(Uuid))`: If validation succeeds (either global or matched scoped).
    /// * `Ok(None)`: Only if in a global context and no ID was requested.
    /// * `Err(CoreError::Auth)`: If the user is scoped and fails the validation checks.
    pub fn validate_workspace(
        &self,
        requested_workspace_id: Option<Uuid>,
    ) -> CoreResult<Option<Uuid>> {
        let ctx = self.ctx;
        let is_global_context = ctx.is_global_workspace()?;

        if is_global_context {
            // Case 1: Global context (admin/root).
            return Ok(requested_workspace_id);
        }

        // Case 2: Scoped context.
        let ctx_workspace_id = ctx.workspace_id();

        // 2a: Scoped user must provide an ID.
        let requested_workspace_id = match requested_workspace_id {
            Some(id) => id,
            None => return Err(CoreError::Auth("workspace_id required".to_string())),
        };

        // 2b: Provided ID must match the context's ID.
        if ctx_workspace_id != requested_workspace_id {
            return Err(CoreError::Auth("unauthorized workspace".to_string()));
        }

        // 2c: Success. The scoped user is operating within their assigned workspace.
        Ok(Some(requested_workspace_id))
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use crate::dev::init::init_test;

    use super::*;

    fn setup_checker() -> CoreResult<PermissionChecker> {
        PermissionChecker::from_str_slice(&[
            "project:read",
            "project:create",
            "account:*",
            "*:read",
        ])
    }

    #[tokio::test]
    #[serial]
    async fn test_validate_perms() -> CoreResult<()> {
        let app = init_test().await;
        let granted = setup_checker()?;

        let mut ctx = CoreCtx::new_test()?;
        ctx.extend_perms(&["account:create"])?;
        let auth = AuthValidator::new(&ctx);

        let success = auth.validate_ctx_perms(&["account:create"]);

        assert!(
            matches!(success, Ok(())),
            "should be success on validate context"
        );
        Ok(())
    }
}

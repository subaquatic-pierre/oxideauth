use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use time::Duration;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    cache::{
        entities::{
            auth::{AuthCache, AuthScopeCache},
            oauth_state::{OAuthProvider, OAuthStateCache},
            workspace::WorkspaceCache,
        },
        manager::CacheManager,
        traits::CacheExecutor,
    },
    config::Config,
    core::{
        ctx::{ContextFactory, CoreCtx},
        email::normalize_email,
        error::{CoreError, CoreResult},
        models::{
            account::{
                Account, AccountCreateParams, AccountDescribeParams, AccountKind, AccountMeta,
                AccountUpdateParams,
            },
            auth::{
                ConfirmParams, LoginParams, OAuthCallbackParams, OAuthInitiateParams,
                RefreshParams, RegisterParams, ResendConfirmParams, ResetPasswordParams,
                RevokeParams, UpdatePasswordParams,
            },
            credential::{
                Credential, CredentialConfig, CredentialCreateParams, CredentialFilter,
                CredentialListParams, CredentialUpdateParams,
            },
            list::RequestFilterParams,
            membership::{
                Membership, MembershipCreateParams, MembershipDescribeParams, MembershipMeta,
                MembershipUpdateParams,
            },
            permission::{PermissionRule, PermissionSet},
            profile::ProfileCreateParams,
            role::{RoleDescribeIdentifier, RoleListParams},
            token::{TokenClaims, TokenType},
            workspace::WorkspaceDescribeParams,
        },
        services::{
            account::AccountService, credential::CredentialService, membership::MembershipService,
            permission::CANONICAL_PERMISSIONS, profile::ProfileService, role::RoleService,
            token::TokenService, validator::AuthValidator, workspace::WorkspaceService,
        },
        traits::{
            params::ValidateParams,
            service::{
                CoreModelCreateService, CoreModelDescribeService, CoreModelListService,
                CoreModelService, CoreModelUpdateService,
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
        error::StoreError,
        manager::StoreManager,
        stores::workspace::SYSTEM_CONST,
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
    profile_svc: Arc<ProfileService<D, C>>,
    ctx_factory: Arc<ContextFactory>,
    role_svc: Arc<RoleService<D, C>>,
    cm: Arc<CacheManager<C>>,
    config: Config,
    validator: Arc<AuthValidator>,
}

struct AuthSvcCreateAccParams {
    // credential fields
    pub cred_kind: CredentialKind,
    pub provider: CredentialProvider,
    pub provider_id: Option<String>,
    pub secret: Option<String>,

    // account fields
    pub email: String,
    pub name: String,
    pub acc_kind: AccountKind,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub verified: bool,

    pub mem_status: MembershipStatus,
}

struct AuthSvcCreateAccRet {
    account: Account,
    membership: Membership,
    credential: Credential,
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
        profile_svc: Arc<ProfileService<D, C>>,
        ctx_factory: Arc<ContextFactory>,
        role_svc: Arc<RoleService<D, C>>,
        config: Config,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            acc_svc,
            ws_svc,
            ctx_factory,
            token_svc,
            credential_svc,
            membership_svc,
            profile_svc,
            role_svc,
            cm,
            config,
            validator,
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
        mem_ver: i64,
        acc_ver: i64,
        sid: Uuid,
        ttl: i64,
    ) -> CoreResult<TokenPair> {
        let now = now_utc();

        let access_claims = TokenClaims::new(
            account_id,
            ws_id,
            mem_id,
            now + Duration::seconds(ttl as i64),
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
            now + Duration::seconds(ttl as i64),
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

    async fn cleanup_failed_onboarding(&self, ctx: &CoreCtx, account_id: Uuid) {
        if let Err(cleanup_err) = self
            .acc_svc
            .cleanup_onboarding_account(ctx, account_id)
            .await
        {
            tracing::error!(%account_id, error = ?cleanup_err, "failed compensating cleanup for onboarding account");
        }
    }

    async fn fetch_ws_cache(
        &self,
        ctx: &CoreCtx,
        ws_id: Option<Uuid>,
        slug: Option<String>,
    ) -> CoreResult<WorkspaceCache> {
        let mut ctx = CoreCtx::bootstrap()?;

        let params = WorkspaceDescribeParams {
            id: ws_id,
            slug: slug,
        };

        let ws_cache = match ws_id {
            Some(ws_id) => {
                let ws_cache = match self.cm.workspace.fetch_by_id(ws_id).await? {
                    Some(ws) => ws,
                    None => self.ws_svc.get_and_cache(&mut ctx, &params).await?.into(),
                };
                ws_cache
            }

            None => self
                .ws_svc
                .get_workspace_by_slug_or_id(&ctx, &params)
                .await?
                .into(),
        };

        Ok(ws_cache)
    }

    async fn create_account(
        &self,
        ctx: &mut CoreCtx,
        params: AuthSvcCreateAccParams,
    ) -> CoreResult<AuthSvcCreateAccRet> {
        let ws_id = ctx.scoped_ws_id();

        let viewer_role = self
            .role_svc
            .get_by_name(
                ctx,
                &RoleDescribeIdentifier {
                    id: None,
                    name: Some(SYSTEM_CONST.workspace_viewer_role.to_string()),
                },
            )
            .await?;

        let account_create_params = AccountCreateParams {
            email: params.email.clone(),
            name: params.name,
            description: None,
            avatar_url: params.avatar_url,
            kind: AccountKind::User,
            verified: true,
            enabled: true,
            tags: None,
            meta: Some(AccountMeta {
                schema_version: "1".to_string(),
            }),
        };

        let account: Account = self
            .acc_svc
            .store()
            .create(&ctx.unscoped_store_ctx(), account_create_params.into())
            .await?
            .into();

        let profile = match self
            .profile_svc
            .create(
                ctx,
                ProfileCreateParams {
                    account_id: account.id,
                    workspace_id: Some(ctx.scoped_ws_id()),
                    email: account.email.clone(),
                    name: account.name.clone(),
                    description: None,
                    display_name: None,
                    job_title: None,
                    timezone: None,
                    avatar_url: account.avatar_url.clone(),
                    tags: vec![],
                    meta: Default::default(),
                },
            )
            .await
        {
            Ok(profile) => profile,
            Err(err) => {
                self.cleanup_failed_onboarding(ctx, account.id).await;
                return Err(err);
            }
        };

        let membership = match self
            .membership_svc
            .create(
                ctx,
                MembershipCreateParams {
                    account_id: account.id,
                    workspace_id: Some(ws_id),
                    profile_id: profile.id,
                    // TODO: derive status, scope, project_id from params
                    status: params.mem_status,
                    scope: MembershipScope::Workspace,
                    project_id: None,
                    role_ids: vec![viewer_role.id.into()],
                    policy_ids: vec![],
                    tags: vec![],
                    meta: MembershipMeta {
                        schema_version: "1".to_string(),
                    },
                },
            )
            .await
        {
            Ok(membership) => membership,
            Err(err) => {
                self.cleanup_failed_onboarding(ctx, account.id).await;
                return Err(err);
            }
        };

        let credential = match self
            .credential_svc
            .create(
                ctx,
                CredentialCreateParams {
                    account_id: account.id,
                    workspace_id: Some(ws_id),
                    membership_id: membership.id,
                    kind: CredentialKind::OAuth,
                    provider: CredentialProvider::Google,
                    status: CredentialStatus::Active,
                    provider_id: params.provider_id,
                    email: Some(params.email),
                    // TODO: get config from workspace
                    config: CredentialConfig::default(),
                    secret: None,
                    expires_at: None,
                    last_used_at: Some(now_utc()),
                    tags: vec![],
                    meta: CredentialMeta {
                        schema_version: "1".to_string(),
                    },
                },
            )
            .await
        {
            Ok(cred) => cred.into(),
            Err(e) => {
                self.cleanup_failed_onboarding(ctx, account.id).await;
                return Err(e);
            }
        };

        Ok(AuthSvcCreateAccRet {
            account,
            membership,
            credential,
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
            workspace,
        } = params.validate()?;

        // get and scope workspace
        let ws_cache = self
            .fetch_ws_cache(ctx, workspace.id, workspace.slug)
            .await?;
        let ws_id = ws_cache.id;
        let ttl = ws_cache.config.jwt_max_age;
        // ensure only public workspaces accept open registrations
        // FEATURE: allow for registration link verification with JWT
        if !ws_cache.config.public {
            return Err(CoreError::Auth(
                "Cannot register to private workspace".into(),
            ));
        }
        ctx.set_scoped_ws(ws_cache);
        ctx.escalate_perms(&[
            CANONICAL_PERMISSIONS.account.create,
            CANONICAL_PERMISSIONS.credential.create,
            CANONICAL_PERMISSIONS.membership.create,
            CANONICAL_PERMISSIONS.profile.create,
        ])?;

        // --- Check email uniqueness ---
        if self
            .acc_svc
            .get_by_email(
                ctx,
                &AccountDescribeParams {
                    id: None,
                    email: Some(email.clone()),
                },
            )
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists(format!(
                "account with email '{}' already exists",
                email
            )));
        }

        // create account

        // --- Look up the default "Workspace Viewer" role ---
        let viewer_role = self
            .role_svc
            .get_by_name(
                ctx,
                &RoleDescribeIdentifier {
                    id: None,
                    name: Some(SYSTEM_CONST.workspace_viewer_role.to_string()),
                },
            )
            .await?;

        // --- Hash password ---
        let secret = hash_password(&password)?;

        // --- Create account (via store — AccountService::create is too heavy with validation) ---
        let default_avatar = format!("https://www.gravatar.com/avatar/{}?d=identicon", "default");
        let params = AuthSvcCreateAccParams {
            email: email.clone(),
            name: name.unwrap_or_else(|| "".to_string()),
            description: None,
            avatar_url: Some(default_avatar),
            verified: false,
            acc_kind: AccountKind::User,
            cred_kind: CredentialKind::Password,
            provider: CredentialProvider::Local,
            provider_id: None,
            // TODO: get default membership status from workspace.config
            mem_status: MembershipStatus::Invited,
            secret: Some(secret),
        };

        let AuthSvcCreateAccRet {
            account,
            membership,
            credential,
        } = self.create_account(ctx, params).await?;

        // let account_row = self.acc_svc.create(ctx, account_create_params).await?;
        // let acc_ver = account_row.version;
        // let account: Account = account_row.into();

        // // Profile identity is established explicitly before the membership.
        // let profile = match self
        //     .profile_svc
        //     .create(
        //         ctx,
        //         ProfileCreateParams {
        //             account_id: account.id,
        //             workspace_id: Some(ws_id),
        //             email: account.email.clone(),
        //             name: account.name.clone(),
        //             description: None,
        //             display_name: None,
        //             job_title: None,
        //             timezone: None,
        //             avatar_url: account.avatar_url.clone(),
        //             tags: vec![],
        //             meta: Default::default(),
        //         },
        //     )
        //     .await
        // {
        //     Ok(profile) => profile,
        //     Err(err) => {
        //         self.cleanup_failed_onboarding(ctx, account.id).await;
        //         return Err(err);
        //     }
        // };

        // // --- Create membership with Viewer role (via service) ---
        // let membership = match self
        //     .membership_svc
        //     .create(
        //         ctx,
        //         MembershipCreateParams {
        //             account_id: account.id,
        //             workspace_id: Some(ws_id),
        //             profile_id: profile.id,
        //             scope: MembershipScope::Workspace,
        //             status: Some(MembershipStatus::Active),
        //             project_id: None,
        //             role_ids: vec![viewer_role.id.into()],
        //             policy_ids: vec![],
        //             tags: vec![],
        //             meta: MembershipMeta {
        //                 schema_version: "1".to_string(),
        //             },
        //         },
        //     )
        //     .await
        // {
        //     Ok(membership) => membership,
        //     Err(err) => {
        //         self.cleanup_failed_onboarding(ctx, account.id).await;
        //         return Err(err);
        //     }
        // };

        // // --- Create credential anchored to the membership (via service) ---
        // if let Err(err) = self
        //     .credential_svc
        //     .create(
        //         ctx,
        //         CredentialCreateParams {
        //             account_id: account.id,
        //             workspace_id: Some(ws_id),
        //             membership_id: membership.id,
        //             kind: CredentialKind::Password,
        //             provider: CredentialProvider::Local,
        //             status: CredentialStatus::Active,
        //             secret: Some(secret),
        //             expires_at: None,
        //             email: Some(email.clone()),
        //             // TODO: get default credential from workspace config
        //             config: CredentialConfig::default(),
        //             provider_id: None,
        //             last_used_at: None,
        //             tags: vec![],
        //             meta: CredentialMeta {
        //                 schema_version: "1".to_string(),
        //             },
        //         },
        //     )
        //     .await
        // {
        //     self.cleanup_failed_onboarding(ctx, account.id).await;
        //     return Err(err);
        // }

        // --- Issue token pair with real workspace + membership IDs ---
        let sid = Uuid::new_v4();
        let tp = self.issue_token_pair(
            account.id,
            ws_id,
            membership.id,
            0, // mem_ver: initial version
            account.version,
            sid,
            ttl,
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

    fn validate_account(account: &Account) -> CoreResult<()> {
        // --- Check account status ---

        if !account.enabled {
            info!(email = %account.email, reason = "account disabled", "AUTH_LOGIN_FAILED");
            return Err(CoreError::Auth("account disabled".to_string()));
        }

        if !account.verified {
            info!(email = %account.email, reason = "account not verified", "AUTH_LOGIN_FAILED");
            return Err(CoreError::Auth("account disabled".to_string()));
        }

        Ok(())
    }

    /// Logs an account in via email/password and returns the account along
    /// with a token pair (access + refresh).
    ///
    /// Login is scoped to a workspace: `params.workspace` is a typed id-or-slug
    /// descriptor, and the account's `Local` password credential is looked up
    /// within that workspace.
    pub async fn login(&self, ctx: &mut CoreCtx, params: LoginParams) -> CoreResult<AuthResult> {
        debug!("LOGIN PARAMS: {params:?}");

        let LoginParams {
            email,
            password,
            workspace,
        } = params.validate()?;

        debug!("LOGIN PARAMS: {email}, {password}, {workspace:?}");

        // --- Resolve the target workspace (id or slug) ---
        let ws = self
            .ws_svc
            .get_workspace_by_slug_or_id(ctx, &workspace)
            .await?;
        let ws_id = ws.id;

        // if workspace not found then fail
        // scope this context to request workspace
        ctx.set_scoped_ws(ws.into());
        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.account.describe])?;

        // --- Find account by email ---
        let account_row = match self
            .acc_svc
            .get_by_email(
                ctx,
                &AccountDescribeParams {
                    id: None,
                    email: Some(email.clone()),
                },
            )
            .await?
        {
            Some(row) => row,
            None => {
                info!(
                    email = %email,
                    reason = "account not found",
                    "AUTH_LOGIN_FAILED"
                );
                return Err(CoreError::Auth("invalid credentials".to_string()));
            }
        };
        let acc_ver = account_row.version;
        let account: Account = account_row.into();
        Self::validate_account(&account)?;

        // --- Find Local password credential for the account in this workspace ---
        let filter: CredentialFilter = json!({
            "account_id": account.id.to_string(),
            "provider": CredentialProvider::Local.to_string(),
            "workspace_id": ws_id.to_string()
        })
        .try_into()?;

        let credentials = self
            .credential_svc
            .store()
            .list(&ctx.unscoped_store_ctx(), Some(filter), None)
            .await?;

        let credential = credentials
            .into_iter()
            .find(|c| c.provider == CredentialProvider::Local && c.secret.is_some())
            .ok_or_else(|| {
                info!(
                    email = %email,
                    reason = "credential not found",
                    "AUTH_LOGIN_FAILED"
                );
                CoreError::Auth("invalid credentials".to_string())
            })?;

        let secret = credential.secret.as_deref().ok_or_else(|| {
            info!(
                email = %email,
                reason = "invalid credentials, no secret",
                "AUTH_LOGIN_FAILED"
            );
            CoreError::Auth("invalid credentials".to_string())
        })?;

        // --- Verify password ---
        if !verify_password(secret, &password)? {
            info!(
                email = %email,
                reason = "invalid password",
                "AUTH_LOGIN_FAILED"
            );
            return Err(CoreError::Auth("invalid credentials".to_string()));
        }

        // --- Resolve the membership the credential is anchored to ---
        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.membership.describe])?;
        let membership = self
            .membership_svc
            .describe(
                ctx,
                MembershipDescribeParams {
                    id: credential.membership_id.into(),
                    workspace_id: Some(credential.workspace_id.into()),
                },
            )
            .await
            .map_err(|e| {
                info!(
                    email = %email,
                    reason = "invalid credentials",
                    "AUTH_LOGIN_FAILED"
                );
                match e {
                    CoreError::StoreError(StoreError::EntityNotFound { .. }) => {
                        CoreError::Auth("invalid credentials".to_string())
                    }
                    e => e,
                }
            })?;

        // check if membership active
        if (membership.status != MembershipStatus::Active) {
            info!(
                email = %email,
                reason = "membership is not active status",
                "AUTH_LOGIN_FAILED"
            );
            return Err(CoreError::Auth("invalid credentials".to_string()));
        }

        let ttl = ctx.ws_cache.config.jwt_max_age;
        let mem_id = membership.id;
        let mem_ver = membership.version;

        // --- Issue token pair (access + refresh) ---
        let sid = Uuid::new_v4();
        let tp = self.issue_token_pair(
            account.id,
            credential.workspace_id.into(), // workspace
            mem_id,                         // membership, set up separately
            mem_ver,
            acc_ver,
            sid,
            ttl,
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

    /// Revokes the given bearer token (access or refresh).
    ///
    /// Decodes the token to recover its claims, validates that the caller has
    /// the `auth:revoke` permission, bumps the membership version in the
    /// database, and purges the auth cache. No tokens carrying the old version
    /// will validate after this call.
    pub async fn revoke_token(&self, ctx: &mut CoreCtx, params: RevokeParams) -> CoreResult<bool> {
        let RevokeParams { token } = params;
        let claims = self.token_svc.decode_token_str(&token)?;

        let sid = claims.require_sid()?;
        let mem_id = claims.mem;
        let acc_id = claims.sub;
        let ws_id = claims.ws;

        // Permission check: caller must have auth:revoke
        self.validator
            .validate_ctx_perms(ctx, &[CANONICAL_PERMISSIONS.auth.revoke])?;

        // Extend permissions needed to bump the membership version
        ctx.escalate_perms(&[
            CANONICAL_PERMISSIONS.membership.describe,
            CANONICAL_PERMISSIONS.membership.update,
        ])?;

        // Bump version + invalidate cache
        self.bump_membership_version_and_invalidate(ctx, mem_id, ws_id, acc_id, Some(sid))
            .await?;

        info!(account_id = %ctx.account_id(), sid = %sid, "AUTH_TOKEN_REVOKED");

        Ok(true)
    }

    /// Bumps the membership version by 1 in the database and invalidates
    /// the auth cache. The provided context must already have
    /// membership:describe and membership:update permissions.
    async fn bump_membership_version_and_invalidate(
        &self,
        ctx: &mut CoreCtx,
        mem_id: Uuid,
        ws_id: Uuid,
        acc_id: Uuid,
        sid: Option<Uuid>,
    ) -> CoreResult<()> {
        // Describe membership to get current version
        let membership = self
            .membership_svc
            .describe(
                ctx,
                MembershipDescribeParams {
                    id: mem_id,
                    workspace_id: Some(ws_id),
                },
            )
            .await?;

        // Bump version by 1
        self.membership_svc
            .update(
                ctx,
                MembershipUpdateParams {
                    id: mem_id,
                    workspace_id: Some(ws_id),
                    ..Default::default()
                },
            )
            .await?;

        // Invalidate Redis cache
        // TODO: invalidate auth cache

        Ok(())
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
    pub async fn refresh_token(
        &self,
        ctx: &mut CoreCtx,
        params: RefreshParams,
    ) -> CoreResult<TokenPair> {
        let RefreshParams { token } = params;

        // Decode the refresh token.
        let claims = self.token_svc.decode_token_str(&token)?;
        let validated = claims.validate_refresh()?;
        let sid = validated.sid;
        let jti = validated.jti;
        let acc_id = validated.sub;
        let ws_id = validated.ws;
        let mem_id = validated.mem;

        let account = self
            .acc_svc
            .describe(
                ctx,
                AccountDescribeParams {
                    id: Some(acc_id),
                    email: None,
                },
            )
            .await?;
        Self::validate_account(&account)?;

        let mut ctx = self.ctx_factory.system()?;

        let ws_cache = match self.cm.workspace.fetch_by_id(ws_id).await? {
            Some(ws) => ws,
            None => {
                let ws = self
                    .ws_svc
                    .get_and_cache(
                        &ctx,
                        &WorkspaceDescribeParams {
                            id: Some(ws_id),
                            slug: None,
                        },
                    )
                    .await?;
                ws.into()
            }
        };

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
            .check_and_consume(jti, sid, remaining_ttl as i64)
            .await?
        {
            // REPLAY DETECTED: the refresh token was already used. Bump the
            // membership version so all existing tokens for this session are
            // permanently invalidated, then purge the auth cache.
            {
                // Build a minimal context with the needed permissions.
                // We know the token's claims are valid (we just decoded them),
                // so we can construct a temporary context from the claims.
                // let auth_cache = AuthCache::from_claims(&claims);
                // let ws_cache = WorkspaceCache::new_keyed(&claims.ws);
                // let mut temp_ctx = CoreCtx::new(auth_cache, ws_id)?;
                ctx.escalate_perms(&[
                    CANONICAL_PERMISSIONS.membership.describe,
                    CANONICAL_PERMISSIONS.membership.update,
                ]);

                self.bump_membership_version_and_invalidate(
                    &mut ctx,
                    mem_id,
                    ws_id,
                    acc_id,
                    Some(sid),
                )
                .await?;
            }

            return Err(CoreError::Auth(
                "session compromised, please re-authenticate".to_string(),
            ));
        }

        // --- Issue the next token pair (same session, new jtis) ---
        let now = now_utc();
        let ttl = ws_cache.config.jwt_max_age;

        let access_claims = TokenClaims::new(
            acc_id,
            ws_id,
            mem_id,
            now + Duration::seconds(ttl as i64),
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

    /// Requests a password reset for the account identified by `params.account`.
    ///
    /// `account` is a typed id-or-email descriptor: when an `id` is present the
    /// account is resolved by id, otherwise by email.
    pub async fn request_password_reset(
        &self,
        ctx: &mut CoreCtx,
        params: ResetPasswordParams,
    ) -> CoreResult<()> {
        let params = params.validate()?;

        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.account.describe])?;
        let store = self.acc_svc.store();

        // Resolve the account: by id if present, else by email.
        let account_row = match params.account.id {
            Some(id) => store.get(&ctx.unscoped_store_ctx(), &id.into()).await?,
            None => {
                // Anti-enumeration: silently succeed if the account does not exist.
                let Some(email) = params.account.email else {
                    return Err(CoreError::InvalidParams("ID or email required".to_string()));
                };
                let email = email.trim().to_lowercase();
                match store
                    .get_by_email(&ctx.unscoped_store_ctx(), &email)
                    .await?
                {
                    Some(row) => row,
                    None => return Ok(()),
                }
            }
        };
        let account: Account = account_row.into();
        Self::validate_account(&account)?;

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

        // TODO: send the reset email fire-and-forget.
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
        info!("password reset token for {}: {token}", account.email);

        Ok(())
    }

    /// Updates an account's password using a valid `PasswordReset` token.
    ///
    /// Returns the id of the account whose password was updated.
    pub async fn update_password(
        &self,
        ctx: &mut CoreCtx,
        params: UpdatePasswordParams,
    ) -> CoreResult<Uuid> {
        let UpdatePasswordParams {
            token,
            new_password,
        } = params.validate()?;

        // Decode and validate the token.
        let claims = self.token_svc.decode_token_str(&token)?;
        let account_id = claims.validate_password_reset()?;

        ctx.escalate_perms(&[
            CANONICAL_PERMISSIONS.account.describe,
            CANONICAL_PERMISSIONS.credential.describe,
            CANONICAL_PERMISSIONS.credential.list,
            CANONICAL_PERMISSIONS.credential.update,
        ])?;
        let store = self.acc_svc.store();

        // Load the account and reject disabled accounts.
        let account_row = store
            .get(&ctx.unscoped_store_ctx(), &account_id.into())
            .await?;
        let account: Account = account_row.into();
        Self::validate_account(&account)?;

        // Hash the new password.
        let secret = hash_password(&new_password)?;

        // Find the account's Local password credential (cross-workspace query via store).
        let filter: CredentialFilter = json!({
            "account_id": account.id.to_string(),
            "provider": CredentialProvider::Local.to_string()
        })
        .try_into()?;

        let credentials = self
            .credential_svc
            .store()
            .list(&ctx.unscoped_store_ctx(), Some(filter), None)
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
            workspace_id: Some(credential.workspace_id.into()),
            kind: None,
            provider: None,
            status: None,
            new_provider_id: None,
            new_email: None,
            secret: Some(secret),
            expires_at: None,
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
        params: ConfirmParams,
    ) -> CoreResult<AccountConfirmation> {
        let ConfirmParams { token } = params;

        // Decode and validate the token.
        let claims = self.token_svc.decode_token_str(&token)?;
        let account_id = claims.validate_account_confirm()?;

        ctx.escalate_perms(&[
            CANONICAL_PERMISSIONS.account.describe,
            CANONICAL_PERMISSIONS.account.update,
        ])?;
        let store = self.acc_svc.store();

        let account_row = store
            .get(&ctx.unscoped_store_ctx(), &account_id.into())
            .await?;
        let account: Account = account_row.into();

        // Reject accounts that have already been verified.
        if account.verified {
            return Err(CoreError::AlreadyExists(
                "account already verified".to_string(),
            ));
        }

        // Mark the account as verified.
        let update_params = AccountUpdateParams {
            email: None,
            id: None,
            name: None,
            description: None,
            avatar_url: None,
            enabled: None,
            verified: Some(true),
            tags: None,
            meta: None,
        };
        let update_data: AccountForUpdate =
            update_params.into_store_params(Some(account.version + 1));
        store
            .update(&ctx.unscoped_store_ctx(), &account_id.into(), update_data)
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
    pub async fn resend_confirmation(
        &self,
        ctx: &mut CoreCtx,
        params: ResendConfirmParams,
    ) -> CoreResult<()> {
        let params = params.validate()?;

        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.account.describe])?;
        let store = self.acc_svc.store();

        // Resolve the account: by id if present, else by email.
        let account_row = match params.account.id {
            Some(id) => store.get(&ctx.unscoped_store_ctx(), &id.into()).await?,
            None => {
                // Anti-enumeration: silently succeed if the account does not exist.
                let Some(email) = params.account.email else {
                    return Err(CoreError::InvalidParams("ID or email required".to_string()));
                };
                let email = email.trim().to_lowercase();
                match store
                    .get_by_email(&ctx.unscoped_store_ctx(), &email)
                    .await?
                {
                    Some(row) => row,
                    None => return Ok(()),
                }
            }
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

        // TODO: send the confirmation email fire-and-forget.
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
        info!("account confirmation token for {}: {token}", account.email);

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
        params: OAuthInitiateParams,
    ) -> CoreResult<String> {
        let OAuthInitiateParams {
            redirect_url,
            workspace_id,
        } = params;

        // Generate the CSRF token used as the OAuth `state` parameter.
        let csrf_token = Uuid::new_v4().to_string();

        // Persist the OAuth state in Redis (10 minute TTL).
        let oauth_entity = OAuthStateCache {
            csrf_token: csrf_token.clone(),
            redirect_url: redirect_url.to_string(),
            created_at: now_utc().unix_timestamp(),
            provider: OAuthProvider::Google,
            workspace_id,
        };
        self.cm.oauth_state.write(&oauth_entity, 600).await?;

        // TODO: get google_oauth_client_id, google_oauth_redirect_url from ws_cache
        let google_oauth_client_id = &self.config.google_oauth_client_id;
        let google_oauth_redirect_url = &self.config.google_oauth_redirect_url;

        // Build the Google authorization URL.
        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid+email+profile&state={}",
            google_oauth_client_id, google_oauth_redirect_url, csrf_token,
        );

        Ok(auth_url)
    }

    /// Handles the Google OAuth2 callback.
    ///
    /// Validates the `state` parameter against the stored CSRF token, exchanges
    /// the authorization `code` for access/id tokens, fetches the Google user
    /// profile, and resolves identity credential-first: an active Google
    /// credential is the authoritative link from the provider identity to an
    /// account. On first sign-in the account is onboarded (account, then
    /// workspace membership, then the Google `OAuth` credential anchored to
    /// that membership). Returns the account, a freshly issued token pair, and
    /// the client redirect URL recovered from the stored OAuth state.
    pub async fn process_google_callback(
        &self,
        ctx: &mut CoreCtx,
        params: OAuthCallbackParams,
    ) -> CoreResult<OAuthCallbackResult> {
        let OAuthCallbackParams { code, state } = params;

        // 1. Validate the CSRF state persisted during initiation.
        let oauth_entity = self
            .cm
            .oauth_state
            .fetch_and_consume(&state)
            .await
            .map_err(|_| CoreError::Auth("invalid oauth state".to_string()))?;

        // get and scope workspace cache
        let ws_cache = self
            .fetch_ws_cache(ctx, Some(oauth_entity.workspace_id), None)
            .await?;
        let ws_id = ws_cache.id;
        ctx.escalate_perms(&[
            CANONICAL_PERMISSIONS.account.describe,
            CANONICAL_PERMISSIONS.account.create,
            CANONICAL_PERMISSIONS.credential.create,
            CANONICAL_PERMISSIONS.credential.describe,
            CANONICAL_PERMISSIONS.membership.create,
            CANONICAL_PERMISSIONS.membership.describe,
            CANONICAL_PERMISSIONS.profile.create,
        ])?;
        ctx.set_scoped_ws(ws_cache);

        // 2. Exchange the authorization code for tokens.
        let token_response = request_google_token(&code, &self.config).await?;

        // 3. Fetch the Google user profile.
        let google_user =
            get_google_user(&token_response.access_token, &token_response.id_token).await?;

        if !google_user.verified_email {
            return Err(CoreError::Auth("Google account not verified".to_string()));
        }

        let store = self.acc_svc.store();
        let normalized_email = normalize_email(&google_user.email);

        // 4. Look up an active Google credential by provider identity.
        let store_ctx = ctx.unscoped_store_ctx();
        let credential_filter: CredentialFilter = json!({
            "workspace_id": ws_id.to_string(),
            "kind": CredentialKind::OAuth.to_string(),
            "provider": CredentialProvider::Google.to_string(),
            "provider_id": google_user.id.clone(),
            "status": CredentialStatus::Active.to_string(),
        })
        .try_into()?;
        let list_params = CredentialListParams {
            workspace_id: Some(ws_id),
            filter: Some(RequestFilterParams {
                fields: Some(credential_filter),
                tags: None,
            }),
            options: None,
        };

        let creds = self.credential_svc.list(ctx, list_params).await?;

        // only one credential should match the filter
        if creds.metadata.total > 1 {
            return Err(CoreError::Auth(
                "ambiguous google identity credential".to_string(),
            ));
        }

        // get or create credential
        let credential = creds.data.into_iter().next();

        // if let Some(cred) = credential {

        //     // account with email already exists link
        // } else if let Some(account) = self
        //     .acc_svc
        //     .get_by_email(
        //         ctx,
        //         &AccountDescribeParams {
        //             email: Some(google_user.email),
        //             id: None,
        //         },
        //     )
        //     .await?
        // {
        // }

        // if credential exists, ensure account and membership is valid, generate auth tokens and return

        // no credential found, check if email already exists for google_user.email, if account exists ensure validate account

        // if membership ensure valid membership, generate auth token and return

        // if no membership create membership and credential, return token

        let account_create_params = AuthSvcCreateAccParams {
            email: normalized_email.clone(),
            name: google_user.name,
            description: None,
            avatar_url: google_user.picture.clone(),
            acc_kind: AccountKind::User,
            verified: google_user.verified_email,
            cred_kind: CredentialKind::OAuth,
            provider: CredentialProvider::Google,
            secret: None,
            provider_id: Some(google_user.id),
            // TODO: get membership status from existing account or workspace.config
            mem_status: MembershipStatus::Active,
        };

        let (account, token_ws_id, token_mem_id) = if let Some(credential) = credential {
            // get account for credential
            let account: Account = self
                .acc_svc
                .describe(
                    ctx,
                    AccountDescribeParams {
                        email: None,
                        id: Some(credential.account_id),
                    },
                )
                .await?
                .into();

            // ensure account valid
            if !account.enabled {
                return Err(CoreError::Auth("account disabled".to_string()));
            }

            // get membership for credential
            let membership = self
                .membership_svc
                .describe(
                    ctx,
                    MembershipDescribeParams {
                        id: credential.membership_id.into(),
                        workspace_id: Some(ws_id),
                    },
                )
                .await?;

            // ensure credential matches membership and is valid
            if membership.account_id != credential.account_id
                || membership.workspace_id != ws_id
                || membership.status != MembershipStatus::Active
            {
                return Err(CoreError::Auth(
                    "google identity is not eligible for authentication".to_string(),
                ));
            }
            (account, ws_id, membership.id)
        } else if store
            .get_by_email(&store_ctx, &normalized_email)
            .await?
            .is_some()
        {
            // 5b. The email belongs to an existing account but no Google
            //     credential is linked to it — refuse to authenticate.
            return Err(CoreError::Auth(
                "google identity is not linked to an account".to_string(),
            ));
        } else {
            // (account, ws_id, membership.id)
            todo!()
        };

        // 6. Issue a token pair (access + refresh) for the session.
        let account_row = self
            .acc_svc
            .store()
            .get(&ctx.unscoped_store_ctx(), &account.id.into())
            .await?;
        let acc_ver = account_row.version;

        // NOTE: ctx is not auth scoped

        let sid = Uuid::new_v4();
        let tp = self.issue_token_pair(
            account.id,
            token_ws_id,
            token_mem_id,
            0,
            acc_ver,
            sid,
            // TODO: change this to workspace config
            self.config.access_token_max_age,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{manager::CacheManager, mock::MockChx},
        config::Config,
        core::services::registry::ServiceRegistry,
        store::{
            dbx::MockDbx,
            entities::{account::AccountRow, workspace::WorkspaceRow},
            manager::StoreManager,
        },
    };
    use serial_test::serial;
    use uuid::Uuid;

    /// Builds an `AuthService` (via `ServiceRegistry`) backed by an in-memory
    /// `MockDbx` + `MockChx`, so tests exercise the service logic without a
    /// real database or Redis.
    fn mock_registry(dbx: MockDbx) -> Arc<ServiceRegistry<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        Arc::new(ServiceRegistry::new(&config, sm, cm))
    }

    // region:    --- AuthValidator ---

    #[test]
    fn test_validate_perms_success() -> CoreResult<()> {
        let granted = PermissionSet::from_str_slice(&["account:create", "account:*"])?;
        let res = AuthValidator::new().validate_perms(&granted, &["account:create"]);
        assert!(res.is_ok(), "granted permission should validate");
        Ok(())
    }

    #[test]
    fn test_validate_perms_failure() -> CoreResult<()> {
        let granted = PermissionSet::from_str_slice(&["account:create"])?;
        let res = AuthValidator::new().validate_perms(&granted, &["account:delete"]);
        assert!(
            matches!(res, Err(CoreError::Auth(_))),
            "missing permission should fail validation"
        );
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_validate_ctx_perms() -> CoreResult<()> {
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.account.create])?;
        let auth = AuthValidator::new();

        let success = auth.validate_ctx_perms(&ctx, &[CANONICAL_PERMISSIONS.account.create]);

        assert!(
            matches!(success, Ok(())),
            "should be success on validate context"
        );
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_validate_ctx_perms_failure() -> CoreResult<()> {
        // A workspace-scoped context with no granted permissions.
        let ws_cache = WorkspaceCache {
            id: Uuid::new_v4(),
            slug: "team-ws".to_string(),
            ..WorkspaceCache::default()
        };
        let auth_cache = AuthCache::new_keyed(Uuid::new_v4(), Uuid::new_v4(), None);
        let ctx = CoreCtx::new(auth_cache, ws_cache)?;
        let auth = AuthValidator::new();

        let res = auth.validate_ctx_perms(&ctx, &[CANONICAL_PERMISSIONS.account.create]);

        assert!(
            matches!(res, Err(CoreError::Auth(_))),
            "missing permission should fail context validation"
        );
        Ok(())
    }

    // endregion: --- AuthValidator ---

    // region:    --- AuthService ---

    #[tokio::test]
    #[serial]
    async fn test_issue_token_pair() -> CoreResult<()> {
        // -- Setup
        let registry = mock_registry(MockDbx::new());
        let account_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();
        let sid = Uuid::new_v4();

        // -- Execute
        let pair = registry
            .auth
            .issue_token_pair(account_id, ws_id, mem_id, 1, 2, sid, 900)?;

        let access = registry.token.decode_token_str(&pair.access_token)?;
        let refresh = registry.token.decode_token_str(&pair.refresh_token)?;

        // -- Assert
        assert_eq!(access.sub, account_id);
        assert_eq!(access.ws, ws_id);
        assert_eq!(access.mem, mem_id);
        assert_eq!(access.sid, Some(sid));
        assert_eq!(access.mem_ver, 1);
        assert_eq!(access.acc_ver, 2);
        assert_eq!(access.ty, TokenType::Auth);

        assert_eq!(refresh.ty, TokenType::Refresh);
        assert_eq!(refresh.sid, Some(sid));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_initiate_google_oauth() -> CoreResult<()> {
        // -- Setup
        let registry = mock_registry(MockDbx::new());
        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute
        let url = registry
            .auth
            .initiate_google_oauth(
                &mut ctx,
                OAuthInitiateParams {
                    redirect_url: "http://localhost/callback".to_string(),
                    workspace_id: Uuid::nil(),
                },
            )
            .await?;

        // -- Assert
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth"));
        assert!(url.contains("client_id=mock-client-id"));
        assert!(url.contains("state="), "URL must carry the CSRF state");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_register_private_workspace() -> CoreResult<()> {
        // -- Setup: bootstrap workspace config has `public == false`
        let registry = mock_registry(MockDbx::new());
        let mut ctx = CoreCtx::bootstrap()?;
        let params = RegisterParams {
            email: "user@example.com".to_string(),
            password: "secret".to_string(),
            name: Some("User".to_string()),
            // Empty descriptor: falls back to ctx.ws_cache (bootstrap → private).
            workspace: WorkspaceDescribeParams::default(),
        };

        // -- Execute
        let err = registry.auth.register(&mut ctx, params).await;

        // -- Assert
        assert!(
            matches!(err, Err(CoreError::Auth(_))),
            "registering to a private workspace must fail"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_login_empty_credentials() -> CoreResult<()> {
        // -- Setup
        let registry = mock_registry(MockDbx::new());
        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute / -- Assert
        let err = registry
            .auth
            .login(
                &mut ctx,
                LoginParams {
                    email: "  ".to_string(),
                    password: "secret".to_string(),
                    workspace: WorkspaceDescribeParams {
                        id: None,
                        slug: Some("ws-1".to_string()),
                    },
                },
            )
            .await;
        assert!(matches!(err, Err(CoreError::InvalidParams(_))));

        let err = registry
            .auth
            .login(
                &mut ctx,
                LoginParams {
                    email: "user@example.com".to_string(),
                    password: "".to_string(),
                    workspace: WorkspaceDescribeParams {
                        id: None,
                        slug: Some("ws-1".to_string()),
                    },
                },
            )
            .await;
        assert!(matches!(err, Err(CoreError::InvalidParams(_))));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_request_token() -> CoreResult<()> {
        // -- Setup
        let registry = mock_registry(MockDbx::new());
        let ctx = CoreCtx::bootstrap()?;

        // -- Execute / -- Assert (currently a no-op that always succeeds)
        registry.auth.request_token(&ctx).await?;

        Ok(())
    }

    // endregion: --- AuthService ---
}

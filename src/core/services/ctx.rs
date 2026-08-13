use std::sync::Arc;

use axum::http::HeaderMap;
use reqwest::StatusCode;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    cache::{
        CacheEntity,
        entities::{
            auth::{AuthCache, AuthScopeCache},
            workspace::WorkspaceCache,
        },
        manager::CacheManager,
        traits::CacheExecutor,
    },
    config::Config,
    core::{
        ctx::{ContextFactory, CoreCtx},
        error::{CoreError, CoreResult},
        models::token::TokenClaims,
        services::{registry::ServiceRegistry, token::TokenService},
    },
    store::{
        ctx::StoreCtx,
        entities::membership::MembershipStatus,
        join::GetManyToMany,
        manager::StoreManager,
        stores::workspace::SYSTEM_CONST,
        traits::{crud::Get, dbx::DbExecutor},
    },
    web::error::ErrorBody,
};

/// Header used by global/root tokens to specify which workspace they are
/// operating on. Scoped tokens do not need to send this header — their
/// workspace is resolved from the JWT.

pub struct CtxService<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    svc_reg: Arc<ServiceRegistry<D, C>>,
    config: Config,
}

impl<D, C> CtxService<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    pub fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        svc_reg: Arc<ServiceRegistry<D, C>>,
        config: Config,
    ) -> Self {
        Self {
            sm,
            cm,
            svc_reg,
            config,
        }
    }

    /// Resolves a `CoreCtx` from the bearer token in the request headers.
    ///
    /// # Flow
    ///
    /// 1. Extract and decode the JWT.
    /// 2. Read the version/status/scope cache keys in a single Redis pipeline.
    /// 3. On a cache miss, hydrate the auth cache from the database and write
    ///    it back through the `AuthCacheStore`.
    /// 4. Validate version claims (membership/account/session) and status
    ///    flags, then reconstruct the `CoreCtx` from the cached auth scope.
    pub async fn resolve_ctx(&self, headers: &HeaderMap) -> CoreResult<CoreCtx> {
        // debug!("HEADERS: {:#?}", headers);

        let token_str = TokenService::<D, C>::token_str_from_headers(headers)
            .ok_or_else(|| CoreError::Auth("Missing authorization header".into()))?;

        let token_svc = self.svc_reg.token.clone();
        let claims = token_svc.decode_token_str(token_str)?;

        let resolver =
            ContextResolver::new(self.sm.clone(), self.cm.clone(), &self.config, &claims);
        let ws_cache = resolver.resolve_ws_cache().await?;
        let auth_cache = resolver.resolve_auth_cache(&ws_cache).await?;

        let acc_id = auth_cache.acc_id;
        let mem_id = auth_cache.mem_id;
        let ws_id = ws_cache.id;

        let mut core_ctx = CoreCtx::new(auth_cache, ws_cache)?;

        // scope global workspace if token is system scope
        // this mutates ctx internal state and ensures correct headers are set
        // for workspace in which to operate
        self.validate_and_scope_global_workspace(headers, &mut core_ctx)?;
        let ws_id = core_ctx.scoped_ws_id();

        info!(
            account_id = %acc_id,
            membership_id = %mem_id,
            workspace_id = %ws_id,
            "CTX_RESOLVED"
        );

        Ok(core_ctx)
    }

    fn validate_and_scope_global_workspace(
        &self,
        headers: &HeaderMap,
        ctx: &mut CoreCtx,
    ) -> CoreResult<()> {
        let header_ws = CtxService::<D, C>::parse_workspace_header(headers);
        let is_system = ctx.is_system_workspace().unwrap_or(false);
        let scoped_ws = if is_system {
            match header_ws {
                Some(scoped_ws) => {
                    // System account gets full root permissions.
                    // Other system-workspace members use their normal roles.
                    if ctx.account_id() == self.svc_reg.ctx_factory.system_user_id() {
                        ctx.extend_perms(&["*:*"]);
                    }
                    scoped_ws
                }
                None => {
                    let msg = format!(
                        "{} header required for global-scope tokens",
                        SYSTEM_CONST.workspace_header_key
                    );
                    error!(msg);
                    return Err(CoreError::Auth(msg));
                }
            }
        } else {
            ctx.auth_cache.auth_scope.workspace_id.clone()
        };

        ctx.set_scoped_ws(ctx.ws_cache.clone());
        Ok(())
    }

    /// Parse the `X-Workspace-Id` header as a UUID. Returns `None` if the header
    /// is missing or unparseable.
    pub fn parse_workspace_header(headers: &HeaderMap) -> Option<Uuid> {
        headers
            .get(SYSTEM_CONST.workspace_header_key)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
    }
}

/// Validates cached auth state against the JWT claims.
///
/// All cache I/O has moved to `CtxService` (via `AuthCacheStore`); this
/// resolver now only holds the validation logic. `sm` and `config` are kept
/// for potential future use — the validation methods themselves need no
/// external state.
struct ContextResolver<'a, D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    config: &'a Config,
    claims: &'a TokenClaims,
}

impl<'a, D, C> ContextResolver<'a, D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        config: &'a Config,
        claims: &'a TokenClaims,
    ) -> Self {
        Self {
            sm,
            cm,
            config,
            claims,
        }
    }

    pub async fn resolve_auth_cache(&self, ws_cache: &WorkspaceCache) -> CoreResult<AuthCache> {
        let mem_id = self.claims.mem;
        let acc_id = self.claims.sub;
        let sid = self.claims.sid;
        let ws = self.claims.ws;
        // Build the keyed template, then read the auth cache.
        let keyed = AuthCache::new_keyed(mem_id, acc_id, sid);
        let auth_cache = self.fetch_auth_cache(&keyed, ws_cache).await?;

        self.validate_auth(&auth_cache)?;

        Ok(auth_cache)
    }

    pub async fn resolve_ws_cache(&self) -> CoreResult<WorkspaceCache> {
        let keyed = WorkspaceCache::new_keyed(self.claims.ws);
        let ws_cache = self.fetch_workspace_cache(&keyed).await?;
        Ok(ws_cache)
    }

    async fn fetch_auth_cache(
        &self,
        keyed: &AuthCache,
        ws_cache: &WorkspaceCache,
    ) -> CoreResult<AuthCache> {
        let hydrated = match self.cm.auth.fetch(&keyed.key()).await? {
            Some(entity) => entity,
            None => {
                let hydrated = AuthCache::build_from_db(
                    self.sm.clone(),
                    keyed.mem_id,
                    keyed.acc_id,
                    keyed.sid,
                )
                .await?;
                self.cm
                    .auth
                    // TODO: need to get workspace config for workspace max token age
                    .write(&hydrated, Some(ws_cache.config.jwt_max_age))
                    .await?;
                hydrated
            }
        };

        Ok(hydrated)
    }

    async fn fetch_workspace_cache(&self, keyed: &WorkspaceCache) -> CoreResult<WorkspaceCache> {
        let hydrated = match self.cm.workspace.fetch(&keyed.key()).await? {
            Some(entity) => entity,
            None => {
                let hydrated = WorkspaceCache::build_from_db(self.sm.clone(), keyed.id).await?;
                self.cm
                    .workspace
                    // TODO: need to get workspace config for workspace max token age
                    .write(&hydrated, None)
                    .await?;
                hydrated
            }
        };

        Ok(hydrated)
    }

    fn validate_auth(&self, auth: &AuthCache) -> CoreResult<()> {
        self.validate_versions(auth)?;
        self.validate_status(auth)?;
        Ok(())
    }

    fn validate_versions(&self, auth: &AuthCache) -> CoreResult<()> {
        let claims = self.claims;
        if claims.mem_ver != auth.mem_version {
            return Err(CoreError::Auth("membership token revoked".into()));
        }

        if claims.acc_ver != auth.acc_version {
            return Err(CoreError::Auth("account token revoked".into()));
        }

        // If the cached entity carries a session, the token's session must
        // match (session revocation bumps the cached session id to `None`).
        if auth.sid.is_some() && claims.sid != auth.sid {
            return Err(CoreError::Auth("session revoked".into()));
        }
        Ok(())
    }

    fn validate_status(&self, auth: &AuthCache) -> CoreResult<()> {
        // --- Status checks ---
        if !auth.mem_active {
            return Err(CoreError::Auth("membership inactive".into()));
        }
        if !auth.acc_enabled {
            return Err(CoreError::Auth("account disabled".into()));
        }
        Ok(())
    }
}

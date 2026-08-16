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
        self.validate_and_scope_global_workspace(headers, &mut core_ctx)
            .await?;
        let ws_id = core_ctx.scoped_ws_id();

        info!(
            account_id = %acc_id,
            membership_id = %mem_id,
            workspace_id = %ws_id,
            "CTX_RESOLVED"
        );

        Ok(core_ctx)
    }

    async fn validate_and_scope_global_workspace(
        &self,
        headers: &HeaderMap,
        ctx: &mut CoreCtx,
    ) -> CoreResult<()> {
        let header_ws = CtxService::<D, C>::parse_workspace_header(headers);
        let is_system = ctx.is_system_workspace().unwrap_or(false);

        if is_system {
            // NOTE(workspace-scope): scoped — a system-namespace token switches
            // its operational workspace via the X-Workspace-Id header.
            let Some(ws_id) = header_ws else {
                let msg = format!(
                    "{} header required for global-scope tokens",
                    SYSTEM_CONST.workspace_header_key
                );
                error!(msg);
                return Err(CoreError::Auth(msg));
            };

            // Cache-first: avoid a DB round-trip on every request.
            let ws_cache = match self.cm.workspace.fetch_by_id(ws_id).await? {
                Some(ws) => ws,
                None => {
                    let hydrated = WorkspaceCache::build_from_db(self.sm.clone(), ws_id).await?;
                    self.cm.workspace.write(&hydrated, None).await?;
                    hydrated
                }
            };

            ctx.set_scoped_ws(ws_cache);
        }

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{
            entities::auth::{AuthCache, AuthScopeCache},
            entities::workspace::WorkspaceCache,
            manager::CacheManager,
            mock::MockChx,
        },
        config::Config,
        core::{
            models::token::{TokenClaims, TokenType},
            services::{
                permission::CANONICAL_PERMISSIONS, registry::ServiceRegistry,
                validator::AuthValidator,
            },
        },
        store::dbx::MockDbx,
        utils::time::now_utc,
    };
    use axum::http::HeaderMap;
    use serial_test::serial;
    use time::Duration as TimeDuration;
    use uuid::Uuid;

    fn mock_ctx_service() -> CtxService<MockDbx, MockChx> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(MockDbx::new())));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = Arc::new(ServiceRegistry::new(&config, sm.clone(), cm.clone()));
        CtxService::new(sm, cm, svc_reg, config)
    }

    #[tokio::test]
    #[serial]
    async fn test_parse_workspace_header() -> CoreResult<()> {
        // -- Execute: missing header
        let headers = HeaderMap::new();
        // -- Assert
        assert_eq!(
            CtxService::<MockDbx, MockChx>::parse_workspace_header(&headers),
            None
        );

        // -- Execute: valid UUID header
        let ws_id = Uuid::new_v4();
        let mut headers = HeaderMap::new();
        headers.insert(
            SYSTEM_CONST.workspace_header_key,
            ws_id.to_string().parse().unwrap(),
        );
        // -- Assert
        assert_eq!(
            CtxService::<MockDbx, MockChx>::parse_workspace_header(&headers),
            Some(ws_id)
        );

        // -- Execute: unparseable UUID header
        let mut headers = HeaderMap::new();
        headers.insert(
            SYSTEM_CONST.workspace_header_key,
            "not-a-uuid".parse().unwrap(),
        );
        // -- Assert
        assert_eq!(
            CtxService::<MockDbx, MockChx>::parse_workspace_header(&headers),
            None
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_ctx_bootstrap_and_escalate_perms() -> CoreResult<()> {
        // -- Setup
        let mut ctx = CoreCtx::bootstrap()?;

        // -- Execute
        ctx.escalate_perms(&[CANONICAL_PERMISSIONS.account.list])?;
        let auth = AuthValidator::new();

        // -- Assert
        assert!(
            auth.validate_ctx_perms(&ctx, &[CANONICAL_PERMISSIONS.account.list])
                .is_ok()
        );
        assert_eq!(ctx.account_id(), Uuid::nil());
        assert_eq!(ctx.membership_id(), Uuid::nil());
        // The bootstrap workspace uses the system slug, so it is a global context.
        assert!(ctx.is_system_workspace()?);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_workspace_scoping() -> CoreResult<()> {
        // -- Setup
        let ws_id = Uuid::new_v4();
        let mut ctx = CoreCtx::bootstrap()?;
        let auth = AuthValidator::new();

        // -- Execute / -- Assert: system context may scope to any workspace
        assert_eq!(auth.validate_workspace(&ctx, Some(ws_id))?, Some(ws_id));
        let store_ctx = auth.scope_store_workspace(&ctx, Some(ws_id))?;
        assert_eq!(store_ctx.workspace_scope(), Some(ws_id));

        // -- Setup: non-system (workspace-scoped) context
        let ws_cache = WorkspaceCache {
            id: ws_id,
            slug: "team-ws".to_string(),
            ..WorkspaceCache::default()
        };
        let auth_cache = AuthCache::new_keyed(Uuid::new_v4(), Uuid::new_v4(), None);
        let scoped_ctx = CoreCtx::new(auth_cache, ws_cache)?;
        let auth = AuthValidator::new();

        // -- Execute / -- Assert: matching workspace is allowed
        assert_eq!(auth.validate_workspace(&scoped_ctx, Some(ws_id))?, Some(ws_id));
        // Deriving from the context works when no workspace is requested
        assert_eq!(auth.validate_workspace(&scoped_ctx, None)?, Some(ws_id));
        // -- Execute: mismatched workspace is rejected
        let other = Uuid::new_v4();
        assert!(matches!(
            auth.validate_workspace(&scoped_ctx, Some(other)),
            Err(CoreError::Auth(_))
        ));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_context_resolver_validation() -> CoreResult<()> {
        // -- Setup
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(MockDbx::new())));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));

        let acc_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();
        let mem_id = Uuid::new_v4();
        let sid = Uuid::new_v4();
        let claims = TokenClaims::new(
            acc_id,
            ws_id,
            mem_id,
            now_utc() + TimeDuration::hours(1),
            TokenType::Auth,
            1,
            2,
            Some(sid),
            None,
        );
        let resolver = ContextResolver::new(sm, cm, &config, &claims);

        let auth = AuthCache {
            mem_id,
            acc_id,
            sid: Some(sid),
            mem_version: 1,
            acc_version: 2,
            mem_active: true,
            acc_enabled: true,
            auth_scope: AuthScopeCache::default(),
        };

        // -- Execute / -- Assert: all versions + status match
        assert!(resolver.validate_auth(&auth).is_ok());

        // -- Execute: membership version mismatch
        let bad_mem = AuthCache {
            mem_version: 99,
            ..auth.clone()
        };
        assert!(matches!(
            resolver.validate_auth(&bad_mem),
            Err(CoreError::Auth(_))
        ));

        // -- Execute: account version mismatch
        let bad_acc = AuthCache {
            acc_version: 99,
            ..auth.clone()
        };
        assert!(matches!(
            resolver.validate_auth(&bad_acc),
            Err(CoreError::Auth(_))
        ));

        // -- Execute: session revoked (cached sid differs from the claim)
        let bad_sid = AuthCache {
            sid: Some(Uuid::new_v4()),
            ..auth.clone()
        };
        assert!(matches!(
            resolver.validate_auth(&bad_sid),
            Err(CoreError::Auth(_))
        ));

        // -- Execute: inactive membership
        let inactive = AuthCache {
            mem_active: false,
            ..auth.clone()
        };
        assert!(matches!(
            resolver.validate_auth(&inactive),
            Err(CoreError::Auth(_))
        ));

        // -- Execute: disabled account
        let disabled = AuthCache {
            acc_enabled: false,
            ..auth.clone()
        };
        assert!(matches!(
            resolver.validate_auth(&disabled),
            Err(CoreError::Auth(_))
        ));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_resolve_ctx_requires_auth_header() -> CoreResult<()> {
        // -- Setup
        let svc = mock_ctx_service();
        let headers = HeaderMap::new();

        // -- Execute
        let res = svc.resolve_ctx(&headers).await;

        // -- Assert
        assert!(
            matches!(res, Err(CoreError::Auth(_))),
            "resolve_ctx without a bearer token must fail with an Auth error"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_validate_and_scope_global_workspace() -> CoreResult<()> {
        // -- Setup
        let svc = mock_ctx_service();
        let headers = HeaderMap::new();

        // -- Execute: system context without an X-Workspace-Id header is rejected
        let mut system_ctx = CoreCtx::bootstrap()?;
        assert!(matches!(
            svc.validate_and_scope_global_workspace(&headers, &mut system_ctx)
                .await,
            Err(CoreError::Auth(_))
        ));

        // -- Setup: workspace-scoped context (not system)
        let ws_id = Uuid::new_v4();
        let ws_cache = WorkspaceCache {
            id: ws_id,
            slug: "team-ws".to_string(),
            ..WorkspaceCache::default()
        };
        let auth_cache = AuthCache::new_keyed(Uuid::new_v4(), Uuid::new_v4(), None);
        let mut scoped_ctx = CoreCtx::new(auth_cache, ws_cache)?;

        // -- Execute: scoped context resolves to its own workspace, no header needed
        assert!(
            svc.validate_and_scope_global_workspace(&headers, &mut scoped_ctx)
                .await
                .is_ok()
        );
        assert_eq!(scoped_ctx.scoped_ws_id(), ws_id);

        Ok(())
    }
}

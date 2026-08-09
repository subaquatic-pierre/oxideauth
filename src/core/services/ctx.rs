use std::sync::Arc;

use axum::http::HeaderMap;
use reqwest::StatusCode;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    cache::{
        entities::auth::{AuthCache, AuthScopeCache},
        manager::CacheManager,
        traits::CacheExecutor,
    },
    config::Config,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::token::TokenClaims,
        services::{factory::ServiceFactory, token::TokenService},
    },
    dev::fixtures::{global_ws_id, root_user_id},
    store::{
        ctx::StoreCtx,
        entities::membership::MembershipStatus,
        join::GetManyToMany,
        manager::StoreManager,
        traits::{crud::Get, dbx::DbExecutor},
    },
    web::error::ErrorBody,
};

/// Header used by global/root tokens to specify which workspace they are
/// operating on. Scoped tokens do not need to send this header — their
/// workspace is resolved from the JWT.
const WORKSPACE_ID_HEADER: &str = "X-Workspace-Id";

pub struct CtxService<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    svc_factory: Arc<ServiceFactory<D, C>>,
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
        svc_factory: Arc<ServiceFactory<D, C>>,
        config: Config,
    ) -> Self {
        Self {
            sm,
            cm,
            svc_factory,
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

        let token_str = TokenService::<D>::token_str_from_headers(headers)
            .ok_or_else(|| CoreError::Auth("missing authorization header".into()))?;

        let token_svc = self.svc_factory.token();
        let claims = token_svc.decode_token_str(token_str)?;

        let mem_id = claims.mem_id()?;
        let acc_id = claims.acc_id()?;
        let sid = claims.sid();

        // Build the keyed template, then read the auth cache.
        let keyed = AuthCache::new_keyed(mem_id, acc_id, sid);
        let auth_cache = self.fetch_auth_cache(&keyed).await?;

        // Validate version/status claims against the cached state.
        let auth_resolver = AuthContextResolver::new(self.sm.clone(), &self.config);
        auth_resolver.validate(&auth_cache, &claims)?;

        let ws_scope = auth_cache.auth_scope.workspace_id;
        let mut core_ctx = CoreCtx::new(auth_cache, ws_scope)?;

        // scope global workspace if token is global scope
        // this mutates ctx internal state and ensures header is set
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

    async fn fetch_auth_cache(&self, keyed: &AuthCache) -> CoreResult<AuthCache> {
        let hydrated = match self.cm.auth.fetch(&keyed).await? {
            Some(entity) => entity,
            None => {
                let hydrated = self
                    .build_auth_cache_from_db(keyed.mem_id, keyed.acc_id, keyed.sid)
                    .await?;
                self.cm
                    .auth
                    .write(&hydrated, self.config.access_token_max_age)
                    .await?;
                hydrated
            }
        };

        Ok(hydrated)
    }

    fn validate_and_scope_global_workspace(
        &self,
        headers: &HeaderMap,
        ctx: &mut CoreCtx,
    ) -> CoreResult<()> {
        let header_ws = CtxService::<D, C>::parse_workspace_header(headers);
        let scoped_ws = if ctx.is_global_workspace().unwrap_or(false) {
            match header_ws {
                Some(scoped_ws) => {
                    // TODO: REMOVE THIS, QUICK HACK FOR DEVELOPMENT PURPOSE
                    // WE NEED TO EXPLICIT ADD PERMISSIONS TO ACCOUNTS/MEMBERSHIPS
                    // THAT ARE IN THE GLOBAL NAMESPACE
                    ctx.extend_perms(&["*:*"]);
                    scoped_ws
                }
                None => {
                    error!("X-Workspace-Id header required for global-scope tokens");
                    return Err(CoreError::Auth(
                        "X-Workspace-Id header required for global-scope tokens".into(),
                    ));
                }
            }
        } else {
            ctx.auth_cache.auth_scope.workspace_id.clone()
        };

        ctx.set_scoped_ws_id(scoped_ws);
        Ok(())
    }

    /// Hydrates a fully-populated `AuthCache` from the database.
    ///
    /// Loads the membership (with its roles), the account, and every role's
    /// permissions, then packages the result for `AuthCacheStore::write`.
    async fn build_auth_cache_from_db(
        &self,
        mem_id: Uuid,
        acc_id: Uuid,
        sid: Option<Uuid>,
    ) -> CoreResult<AuthCache> {
        let store_ctx = StoreCtx::new_root();

        // Load the membership (with its roles).
        let mem_with_roles = self
            .sm
            .membership
            .get_many_to_many(&store_ctx, &mem_id.into())
            .await?;
        let mem_row = mem_with_roles.membership;

        // Load the account.
        let acc_row = self.sm.account.get(&store_ctx, &acc_id.into()).await?;

        // Resolve permissions from the membership's roles.
        let mut permissions: Vec<String> = vec![];
        let mut role_ids: Vec<Uuid> = vec![];
        for role in mem_with_roles.roles.iter() {
            role_ids.push(role.id.into());
            let role_with_perms = self.sm.role.get_many_to_many(&store_ctx, &role.id).await?;
            for perm in role_with_perms.permissions.iter() {
                let code = perm.code.clone().unwrap_or_else(|| perm.name.clone());
                if !permissions.contains(&code) {
                    permissions.push(code);
                }
            }
        }

        let auth_scope = AuthScopeCache {
            workspace_id: mem_row.workspace_id,
            project_id: mem_row.project_id,
            roles: role_ids,
            permissions,
        };

        Ok(AuthCache {
            mem_id,
            acc_id,
            sid,
            mem_version: mem_row.token_version as u64,
            acc_version: acc_row.token_version as u64,
            mem_active: mem_row.status == MembershipStatus::Active,
            acc_enabled: acc_row.enabled,
            auth_scope,
        })
    }

    /// Parse the `X-Workspace-Id` header as a UUID. Returns `None` if the header
    /// is missing or unparseable.
    pub fn parse_workspace_header(headers: &HeaderMap) -> Option<Uuid> {
        headers
            .get(WORKSPACE_ID_HEADER)
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
struct AuthContextResolver<'a, D>
where
    D: DbExecutor,
{
    #[allow(dead_code)]
    sm: Arc<StoreManager<D>>,
    #[allow(dead_code)]
    config: &'a Config,
}

impl<'a, D> AuthContextResolver<'a, D>
where
    D: DbExecutor,
{
    fn new(sm: Arc<StoreManager<D>>, config: &'a Config) -> Self {
        Self { sm, config }
    }

    fn validate(&self, auth: &AuthCache, claims: &TokenClaims) -> CoreResult<()> {
        self.validate_versions(auth, claims)?;
        self.validate_status(auth)?;
        Ok(())
    }

    fn validate_versions(&self, auth: &AuthCache, claims: &TokenClaims) -> CoreResult<()> {
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

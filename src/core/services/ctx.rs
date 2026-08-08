use std::sync::Arc;

use axum::http::HeaderMap;
use tracing::info;
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
        models::{
            account::Account, membership::MembershipCache, token::TokenClaims,
            workspace::Workspace,
        },
        services::{factory::ServiceFactory, token::TokenService},
    },
    store::{
        ctx::StoreCtx,
        entities::membership::MembershipStatus,
        join::GetManyToMany,
        manager::StoreManager,
        traits::{crud::Get, dbx::DbExecutor},
    },
};

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
        let token_str = TokenService::<D>::token_str_from_headers(headers)
            .ok_or_else(|| CoreError::Auth("missing authorization header".into()))?;

        let token_svc = self.svc_factory.token();
        let claims = token_svc.decode_token_str(token_str)?;

        let mem_id = claims.mem_id()?;
        let acc_id = claims.acc_id()?;
        let sid = claims.sid();

        // Build the keyed template, then read the auth cache.
        let keyed = AuthCache::new_keyed(mem_id, acc_id, sid);
        let entity = match self.cm.auth.fetch(&keyed).await? {
            Some(entity) => entity,
            None => {
                let hydrated = self.build_auth_cache_from_db(mem_id, acc_id, sid).await?;
                self.cm
                    .auth
                    .write(&hydrated, self.config.access_token_max_age)
                    .await?;
                hydrated
            }
        };

        // Validate version/status claims against the cached state.
        let auth_resolver = AuthContextResolver::new(self.sm.clone(), &self.config);
        auth_resolver.validate(&entity, &claims)?;

        // Reconstruct the CoreCtx from the cached auth scope.
        let auth_scope = entity.auth_scope;

        let cached_mem = MembershipCache {
            id: mem_id,
            account_id: acc_id,
            workspace_id: auth_scope.workspace_id,
            project_id: auth_scope.project_id,
            role_ids: auth_scope.roles,
            permissions: auth_scope.permissions,
        };

        let account = Account {
            id: acc_id,
            ..Default::default()
        };

        let workspace = Workspace {
            id: auth_scope.workspace_id,
            ..Default::default()
        };

        let core_ctx = CoreCtx::new(cached_mem, account, workspace)?;

        info!(
            account_id = %acc_id,
            membership_id = %mem_id,
            "CTX_RESOLVED"
        );

        Ok(core_ctx)
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

use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
    todo,
};

use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::json;
use tracing::info;
use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    config::Config,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::Account, membership::CachedMembership, token::TokenClaims,
            workspace::Workspace,
        },
        services::{factory::ServiceFactory, token::TokenService},
    },
    store::{
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::membership::MembershipStatus,
        join::GetManyToMany,
        manager::StoreManager,
        traits::{crud::Get, dbx::DbExecutor},
    },
};

/// The cached auth-scope payload persisted under `oxauth:auth_sc:{membership_id}`.
///
/// It carries everything needed to reconstruct a [`CoreCtx`] without hitting
/// the database on every authenticated request.
#[derive(Debug, Deserialize)]
struct AuthScopeCache {
    workspace_id: Uuid,
    project_id: Option<Uuid>,
    roles: Vec<Uuid>,
    permissions: Vec<String>,
}

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
    /// 3. On a cache miss, hydrate the auth cache from the database and retry.
    /// 4. Validate version claims (membership/account/session) and status
    ///    flags, then reconstruct the `CoreCtx` from the cached auth scope.
    pub async fn resolve_ctx(&self, headers: &HeaderMap) -> CoreResult<CoreCtx> {
        let token_str = TokenService::<D, C>::token_str_from_headers(headers)
            .ok_or_else(|| CoreError::Auth("missing authorization header".into()))?;

        let token_svc = self.svc_factory.token();
        let claims = token_svc.decode_token_str(token_str)?;

        let mem_id = claims.mem_id()?;
        let acc_id = claims.acc_id()?;
        let sid = claims.sid();

        let auth_resolver = AuthContextResolver::new(
            self.sm.clone(),
            self.cm.clone(),
            self.svc_factory.clone(),
            &self.config,
        );

        let auth_cache_keys = AuthCacheKeys::new(mem_id, acc_id, sid)?;
        let auth_cache_values = auth_resolver.fetch_auth_cache(&auth_cache_keys).await?;

        auth_resolver.validate_cache(&auth_cache_keys, &auth_cache_values, &claims)?;

        let auth_scope = auth_cache_values.auth_scope;

        let cached_mem = CachedMembership {
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
}

struct AuthCacheKeys {
    mem_id: Uuid,
    acc_id: Uuid,
    sid: Option<Uuid>,

    // cache keys
    mem_version: String,
    acc_version: String,
    mem_active: String,
    acc_enabled: String,
    auth_scope: String,
    sid_key: Option<String>,
}

impl AuthCacheKeys {
    fn new(mem_id: Uuid, acc_id: Uuid, sid: Option<Uuid>) -> CoreResult<Self> {
        let mem_version = format!("oxauth:mem_v:{}", mem_id);
        let acc_version = format!("oxauth:acc_v:{}", acc_id); // account version
        let mem_active = format!("oxauth:mem_act:{}", mem_id); // membership active
        let acc_enabled = format!("oxauth:acc_en:{}", acc_id); // account enabled
        let auth_scope = format!("oxauth:auth_sc:{}", mem_id); // auth scope
        let sid_key = sid.map(|s| format!("oxauth:sid:{}", s)); // session version

        Ok(Self {
            mem_id,
            acc_id,
            sid,
            mem_version,
            acc_version,
            mem_active,
            acc_enabled,
            auth_scope,
            sid_key,
        })
    }

    fn to_set(&self) -> HashSet<&str> {
        let mut set = HashSet::new();
        set.insert(self.mem_version.as_str());
        set.insert(self.acc_version.as_str());
        set.insert(self.mem_active.as_str());
        set.insert(self.acc_enabled.as_str());
        set.insert(self.auth_scope.as_str());

        // If the token has no session id (single-use tokens), skip the session
        // version check entirely (treated as version 0).
        if let Some(sid_key) = &self.sid_key {
            set.insert(sid_key.as_str());
        }

        set
    }
}

pub struct AuthCacheValues {
    // mem_id: Uuid,
    // acc_id: Uuid,
    sid: Option<Uuid>,

    // cache keys
    mem_version: u64,
    acc_version: u64,
    mem_active: bool,
    acc_enabled: bool,
    auth_scope: AuthScopeCache,
}

impl AuthCacheValues {
    fn new(keys: &AuthCacheKeys, values: HashMap<String, Option<String>>) -> CoreResult<Self> {
        let mem_version = values
            .get(&keys.mem_version)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("membership token revoked".into()))?
            .parse()?;

        let acc_version = values
            .get(&keys.acc_version)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("account token revoked".into()))?
            .parse()?;

        let mem_active = values
            .get(&keys.mem_active)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("membership inactive".into()))?
            .parse()?;
        let acc_enabled = values
            .get(&keys.acc_enabled)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("account disabled".into()))?
            .parse()?;

        let as_val = values
            .get(&keys.auth_scope)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("auth scope unavailable".into()))?;
        let auth_scope: AuthScopeCache = serde_json::from_str(as_val)
            .map_err(|_| CoreError::Auth("invalid auth scope".into()))?;

        let mut sid: Option<Uuid> = None;
        if let Some(sid_key) = &keys.sid_key {
            let uuid_str = values
                .get(sid_key)
                .and_then(|v| v.clone())
                .ok_or_else(|| CoreError::Auth("auth scope unavailable".into()))?;
            sid = Some(Uuid::from_str(&uuid_str)?)
        }

        Ok(Self {
            // mem_id: todo!(),
            // acc_id: todo!(),
            sid,
            mem_version,
            acc_version,
            mem_active,
            acc_enabled,
            auth_scope,
        })
    }
}

// struct AuthCache {}
// impl AuthCache {
//     fn keys() -> &'static [&'static str] {
//         &[""]
//     }
// }

struct AuthContextResolver<'a, D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    svc_factory: Arc<ServiceFactory<D, C>>,
    config: &'a Config,
}

impl<'a, D, C> AuthContextResolver<'a, D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        svc_factory: Arc<ServiceFactory<D, C>>,
        config: &'a Config,
    ) -> Self {
        Self {
            sm,
            cm,
            svc_factory,
            config,
        }
    }

    fn validate_cache(
        &self,
        keys: &AuthCacheKeys,
        values: &AuthCacheValues,
        claims: &TokenClaims,
    ) -> CoreResult<()> {
        self.validate_versions(keys, values, claims)?;
        self.validate_status(values)?;
        Ok(())
    }
    fn validate_versions(
        &self,
        keys: &AuthCacheKeys,
        values: &AuthCacheValues,
        claims: &TokenClaims,
    ) -> CoreResult<()> {
        if claims.mem_ver != values.mem_version {
            return Err(CoreError::Auth("membership token revoked".into()));
        }

        if claims.acc_ver != values.acc_version {
            return Err(CoreError::Auth("account token revoked".into()));
        }

        if let Some(sid) = values.sid {
            if claims.sid != values.sid {
                return Err(CoreError::Auth("session revoked".into()));
            }
        }
        Ok(())
    }

    fn validate_status(&self, values: &AuthCacheValues) -> CoreResult<()> {
        // --- 6. Status checks ---
        if !values.mem_active {
            return Err(CoreError::Auth("membership inactive".into()));
        }
        if !values.acc_enabled {
            return Err(CoreError::Auth("account disabled".into()));
        }
        Ok(())
    }

    /// Reads the auth cache keys, hydrating from the database on a miss and
    /// retrying the read.
    async fn fetch_auth_cache(&self, keys: &AuthCacheKeys) -> CoreResult<AuthCacheValues> {
        let key_set = keys.to_set();
        let key_vec = key_set.iter().map(|el| el.as_ref()).collect::<Vec<&str>>();
        let chx = self.cm.executor();
        let mut val_vec = chx.pipeline_get(&key_vec).await?;

        if val_vec.iter().any(|v| v.is_none()) {
            self.hydrate_auth_cache(keys).await?;
            val_vec = chx.pipeline_get(&key_vec).await?;
        }

        let map = key_set
            .into_iter()
            .map(|el| el.to_string())
            .zip(val_vec)
            .collect();
        let auth_cache_vals = AuthCacheValues::new(keys, map)?;

        Ok(auth_cache_vals)
    }

    /// Populates all six auth cache keys from the database.
    ///
    /// TTL is the configured access-token lifetime; entries are refreshed on
    /// each cache miss and invalidated wholesale on revocation.
    async fn hydrate_auth_cache(&self, keys: &AuthCacheKeys) -> CoreResult<()> {
        let membership_id = keys.mem_id;
        let account_id: Uuid = keys.acc_id;
        let sid: Option<Uuid> = keys.sid;
        let store_ctx = StoreCtx::new_root();

        // Load the membership (with its roles).
        let mem_with_roles = self
            .sm
            .membership
            .get_many_to_many(&store_ctx, &membership_id.into())
            .await?;
        let mem_row = mem_with_roles.membership;

        // Load the account.
        let acc_row = self.sm.account.get(&store_ctx, &account_id.into()).await?;

        // Resolve permissions from the membership's roles.
        let mut permissions: Vec<String> = vec![];
        let mut role_ids: Vec<String> = vec![];
        for role in mem_with_roles.roles.iter() {
            role_ids.push(role.id.to_string());
            let role_with_perms = self.sm.role.get_many_to_many(&store_ctx, &role.id).await?;
            for perm in role_with_perms.permissions.iter() {
                let code = perm.code.clone().unwrap_or_else(|| perm.name.clone());
                if !permissions.contains(&code) {
                    permissions.push(code);
                }
            }
        }

        let auth_scope = json!({
            "membership_id": mem_row.id.to_string(),
            "account_id": mem_row.account_id.to_string(),
            "workspace_id": mem_row.workspace_id.to_string(),
            "project_id": mem_row.project_id.map(|p| p.to_string()),
            "roles": role_ids,
            "permissions": permissions,
        });

        let ttl = self.config.access_token_max_age;
        let mem_active = if mem_row.status == MembershipStatus::Active {
            "true"
        } else {
            "false"
        };
        let acc_enabled = if acc_row.enabled { "true" } else { "false" };

        let chx = self.cm.executor();
        chx.set_string(
            &keys.mem_version,
            &mem_row.token_version.to_string(),
            Some(ttl),
        )
        .await?;
        chx.set_string(
            &keys.acc_version,
            &acc_row.token_version.to_string(),
            Some(ttl),
        )
        .await?;

        if let (Some(sid_key), Some(sid)) = (&keys.sid_key, sid) {
            chx.set_string(sid_key, &sid.to_string(), Some(ttl)).await?;
        }
        chx.set_string(&keys.mem_active, mem_active, Some(ttl))
            .await?;
        chx.set_string(&keys.acc_enabled, acc_enabled, Some(ttl))
            .await?;
        chx.set_string(&keys.auth_scope, &auth_scope.to_string(), Some(ttl))
            .await?;

        info!(
            membership_id = %membership_id,
            account_id = %account_id,
            "AUTH_CACHE_HYDRATED"
        );

        Ok(())
    }
}

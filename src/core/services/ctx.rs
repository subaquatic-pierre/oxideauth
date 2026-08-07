use std::{str::FromStr, sync::Arc};

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
            account::Account,
            membership::CachedMembership,
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

/// The cached auth-scope payload persisted under `oxauth:as:{membership_id}`.
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
        // --- 1. Extract bearer token ---
        let token_str = TokenService::<D, C>::token_str_from_headers(headers)
            .ok_or_else(|| CoreError::Auth("missing authorization header".into()))?;

        // --- 2. Decode JWT ---
        let token_svc = self.svc_factory.token();
        let claims = token_svc.decode_token_str(token_str)?;

        let mem_id = Uuid::from_str(claims.mem())
            .map_err(|_| CoreError::Auth("invalid token: membership claim".into()))?;
        let acc_id = Uuid::from_str(claims.sub())
            .map_err(|_| CoreError::Auth("invalid token: subject claim".into()))?;
        let sid = claims.sid();

        // --- 3. Build the auth cache keys ---
        let tv_key = format!("oxauth:tv:{}", mem_id);
        let av_key = format!("oxauth:av:{}", acc_id);
        let ma_key = format!("oxauth:ma:{}", mem_id);
        let ae_key = format!("oxauth:ae:{}", acc_id);
        let as_key = format!("oxauth:as:{}", mem_id);

        let mut keys: Vec<String> = vec![
            tv_key.clone(),
            av_key.clone(),
            ma_key.clone(),
            ae_key.clone(),
            as_key.clone(),
        ];
        // If the token has no session id (single-use tokens), skip the session
        // version check entirely (treated as version 0).
        let sv_key = sid.map(|s| format!("oxauth:sv:{}", s));
        if let Some(sv) = &sv_key {
            keys.push(sv.clone());
        }

        // --- 4. Fetch (hydrating on miss) ---
        let values = self.fetch_auth_cache(&keys, mem_id, acc_id, sid).await?;

        let tv_val = values
            .get(0)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("membership token revoked".into()))?;
        let av_val = values
            .get(1)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("account token revoked".into()))?;
        let ma_val = values
            .get(2)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("membership inactive".into()))?;
        let ae_val = values
            .get(3)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("account disabled".into()))?;
        let as_val = values
            .get(4)
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CoreError::Auth("auth scope unavailable".into()))?;
        let sv_val = sv_key
            .as_ref()
            .map(|_| values.get(5).and_then(|v| v.as_deref()));

        // --- 5. Version comparisons ---
        let tv = tv_val
            .parse::<u64>()
            .map_err(|_| CoreError::Auth("membership token revoked".into()))?;
        if claims.mem_ver != tv {
            return Err(CoreError::Auth("membership token revoked".into()));
        }

        let av = av_val
            .parse::<u64>()
            .map_err(|_| CoreError::Auth("account token revoked".into()))?;
        if claims.acc_ver != av {
            return Err(CoreError::Auth("account token revoked".into()));
        }

        if let (Some(_sv_key), Some(sv_val)) = (sv_key.as_ref(), sv_val.flatten()) {
            let sv = sv_val
                .parse::<u64>()
                .map_err(|_| CoreError::Auth("session revoked".into()))?;
            if claims.sid_ver != sv {
                return Err(CoreError::Auth("session revoked".into()));
            }
        }

        // --- 6. Status checks ---
        if ma_val != "true" && ma_val != "1" {
            return Err(CoreError::Auth("membership inactive".into()));
        }
        if ae_val != "true" && ae_val != "1" {
            return Err(CoreError::Auth("account disabled".into()));
        }

        // --- 7. Reconstruct CoreCtx from the cached auth scope ---
        let auth_scope: AuthScopeCache = serde_json::from_str(as_val)
            .map_err(|_| CoreError::Auth("invalid auth scope".into()))?;

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

    /// Reads the auth cache keys, hydrating from the database on a miss and
    /// retrying the read.
    async fn fetch_auth_cache(
        &self,
        keys: &[String],
        mem_id: Uuid,
        acc_id: Uuid,
        sid: Option<Uuid>,
    ) -> CoreResult<Vec<Option<String>>> {
        let key_refs: Vec<&str> = keys.iter().map(|k| k.as_str()).collect();
        let chx = self.cm.executor();

        let mut values = chx.pipeline_get(&key_refs).await?;

        if values.iter().any(|v| v.is_none()) {
            self.hydrate_auth_cache(mem_id, acc_id, sid).await?;
            values = chx.pipeline_get(&key_refs).await?;
        }

        Ok(values)
    }

    /// Populates all six auth cache keys from the database.
    ///
    /// TTL is the configured access-token lifetime; entries are refreshed on
    /// each cache miss and invalidated wholesale on revocation.
    async fn hydrate_auth_cache(
        &self,
        membership_id: Uuid,
        account_id: Uuid,
        sid: Option<Uuid>,
    ) -> CoreResult<()> {
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
            let role_with_perms = self
                .sm
                .role
                .get_many_to_many(&store_ctx, &role.id)
                .await?;
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
            &format!("oxauth:tv:{}", mem_row.id),
            &mem_row.token_version.to_string(),
            Some(ttl),
        )
        .await?;
        chx.set_string(
            &format!("oxauth:av:{}", acc_row.id),
            &acc_row.token_version.to_string(),
            Some(ttl),
        )
        .await?;
        if let Some(sid) = sid {
            chx.set_string(&format!("oxauth:sv:{}", sid), "0", Some(ttl))
                .await?;
        }
        chx.set_string(&format!("oxauth:ma:{}", mem_row.id), mem_active, Some(ttl))
            .await?;
        chx.set_string(&format!("oxauth:ae:{}", acc_row.id), acc_enabled, Some(ttl))
            .await?;
        chx.set_string(
            &format!("oxauth:as:{}", mem_row.id),
            &auth_scope.to_string(),
            Some(ttl),
        )
        .await?;

        info!(
            membership_id = %membership_id,
            account_id = %account_id,
            "AUTH_CACHE_HYDRATED"
        );

        Ok(())
    }
}

use std::str::FromStr;
use std::sync::Arc;
use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::store::crud::Get;
use crate::store::ctx::StoreCtx;
use crate::store::entities::membership::MembershipStatus;
use crate::store::join::GetManyToMany;
use crate::store::manager::StoreManager;
use crate::store::traits::dbx::DbExecutor;
use crate::{
    cache::{
        error::{CacheError, CacheResult},
        traits::{CacheEntity, CacheKey},
    },
    core::models::token::{RefreshClaims, TokenClaims},
    store::stores::workspace::SYSTEM_CONST,
};

/// The cached auth-scope payload persisted under `oxauth:mem_id:{membership_id}`.
///
/// It carries everything needed to reconstruct a [`CoreCtx`] without hitting
/// the database on every authenticated request.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthScopeCache {
    pub workspace_id: Uuid,
    pub workspace_slug: String,
    pub project_id: Option<Uuid>,
    pub roles: Vec<Uuid>,
    pub permissions: Vec<String>,
}

impl AuthScopeCache {
    pub fn system() -> Self {
        Self {
            workspace_id: Uuid::nil(),
            workspace_slug: SYSTEM_CONST.system_ws_slug.to_string(),
            project_id: None,
            roles: vec![],
            permissions: vec!["*:*".to_string()],
        }
    }
}

impl Default for AuthScopeCache {
    fn default() -> Self {
        Self {
            workspace_id: Uuid::nil(),
            workspace_slug: "default".to_string(),
            project_id: None,
            roles: vec![],
            permissions: vec![],
        }
    }
}

/// The auth cache entity for a single token identity.
///
/// `mem_id`/`acc_id`/`sid` are identifiers fixed at construction time and are
/// used to compute the Redis keys. The remaining fields are cached values that
/// are populated after a `fetch` (cache hit) or hydration (cache miss).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCache {
    // Identifiers (set at construction)
    pub mem_id: Uuid,
    pub acc_id: Uuid,
    pub sid: Option<Uuid>,

    // Cached values (populated after fetch/hydrate)
    pub mem_version: u64,
    pub acc_version: u64,
    pub mem_active: bool,
    pub acc_enabled: bool,
    pub auth_scope: AuthScopeCache,
}

impl AuthCache {
    pub fn new_keyed(mem_id: Uuid, acc_id: Uuid, sid: Option<Uuid>) -> Self {
        Self {
            mem_id,
            acc_id,
            sid,
            mem_version: 0,
            acc_version: 0,
            mem_active: false,
            acc_enabled: false,
            auth_scope: AuthScopeCache::default(),
        }
    }

    pub fn bootstrap() -> Self {
        Self {
            mem_id: Uuid::nil(),
            acc_id: Uuid::nil(),
            sid: None,
            mem_version: 0,
            acc_version: 0,
            mem_active: true,
            acc_enabled: true,
            auth_scope: AuthScopeCache::system(),
        }
    }

    pub fn from_claims(token_claims: &TokenClaims) -> Self {
        AuthCache {
            mem_id: token_claims.mem,
            acc_id: token_claims.sub,
            sid: token_claims.sid,
            mem_version: token_claims.mem_ver,
            acc_version: token_claims.acc_ver,
            mem_active: true,
            acc_enabled: true,
            auth_scope: AuthScopeCache {
                workspace_id: token_claims.ws,
                workspace_slug: String::new(),
                project_id: None,
                roles: vec![],
                permissions: vec![],
            },
        }
    }

    /// Hydrates a fully-populated `AuthCache` from the database.
    ///
    /// Loads the membership (with its roles), the account, and every role's
    /// permissions, then packages the result for `AuthCacheStore::write`.
    pub async fn build_from_db<D: DbExecutor>(
        sm: Arc<StoreManager<D>>,
        mem_id: Uuid,
        acc_id: Uuid,
        sid: Option<Uuid>,
    ) -> CacheResult<AuthCache> {
        let store_ctx = StoreCtx::bootstrap();

        // Load the membership (with its roles).
        let mem_with_roles = sm
            .membership
            .get_many_to_many(&store_ctx, &mem_id.into())
            .await?;
        let mem_row = mem_with_roles.membership;
        let workspace_row = sm
            .workspace
            .get(&store_ctx, &mem_row.workspace_id.into())
            .await?;

        // Load the account.
        let acc_row = sm.account.get(&store_ctx, &acc_id.into()).await?;

        // Resolve permissions from the membership's roles.
        let mut permissions: Vec<String> = vec![];
        let mut role_ids: Vec<Uuid> = vec![];
        for role in mem_with_roles.roles.iter() {
            role_ids.push(role.id.into());
            let role_with_perms = sm.role.get_many_to_many(&store_ctx, &role.id).await?;
            for perm in role_with_perms.permissions.iter() {
                let name = perm.name.clone();
                if !permissions.contains(&name) {
                    permissions.push(name);
                }
            }
        }

        let auth_scope = AuthScopeCache {
            workspace_id: mem_row.workspace_id,
            workspace_slug: workspace_row.slug,
            project_id: mem_row.project_id,
            roles: role_ids,
            permissions,
        };

        Ok(AuthCache {
            mem_id,
            acc_id,
            sid,
            mem_version: mem_row.version as u64,
            acc_version: acc_row.version as u64,
            mem_active: mem_row.status == MembershipStatus::Active,
            acc_enabled: acc_row.enabled,
            auth_scope,
        })
    }
}

impl CacheEntity for AuthCache {
    fn _key() -> (&'static str, &'static str) {
        ("oxauth", "mem_id")
    }

    fn key(&self) -> CacheKey {
        let (prefix, name) = AuthCache::_key();
        CacheKey::new(prefix, name, self.mem_id)
    }

    fn new_key(mem_id: impl Display) -> CacheKey {
        let (prefix, name) = AuthCache::_key();
        CacheKey::new(prefix, name, mem_id)
    }
}

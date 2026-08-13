use std::str::FromStr;
use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    pub fn bootstrap_cache() -> Self {
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

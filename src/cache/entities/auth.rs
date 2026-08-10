use std::collections::HashMap;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cache::{
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheKey},
};

/// The cached auth-scope payload persisted under `oxauth:auth_sc:{membership_id}`.
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
    pub fn root_scope() -> Self {
        Self {
            workspace_id: Uuid::nil(),
            workspace_slug: "global".to_string(),
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
#[derive(Debug, Clone)]
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

    pub fn root_cache() -> Self {
        Self {
            mem_id: Uuid::nil(),
            acc_id: Uuid::nil(),
            sid: None,
            mem_version: 0,
            acc_version: 0,
            mem_active: true,
            acc_enabled: true,
            auth_scope: AuthScopeCache::root_scope(),
        }
    }

    /// Creates a bootstrap cache with nil UUIDs — no pre-existing DB data required.
    ///
    /// Used during seeding/initialization. Same as `root_cache()` but with an
    /// explicit name to convey intent.
    pub fn bootstrap_cache() -> Self {
        Self::root_cache()
    }
}

impl CacheEntity for AuthCache {
    fn keys(&self) -> HashMap<String, CacheKey> {
        let mut map = HashMap::new();
        map.insert(
            "mem_version".into(),
            CacheKey::new("oxauth", "mem_v", self.mem_id),
        );
        map.insert(
            "acc_version".into(),
            CacheKey::new("oxauth", "acc_v", self.acc_id),
        );
        map.insert(
            "mem_active".into(),
            CacheKey::new("oxauth", "mem_act", self.mem_id),
        );
        map.insert(
            "acc_enabled".into(),
            CacheKey::new("oxauth", "acc_en", self.acc_id),
        );
        map.insert(
            "auth_scope".into(),
            CacheKey::new("oxauth", "auth_sc", self.mem_id),
        );
        if let Some(ref sid) = self.sid {
            map.insert("sid".into(), CacheKey::new("oxauth", "sid", sid));
        }
        map
    }

    fn from_raw(raw: HashMap<String, Option<String>>) -> CacheResult<Self> {
        // Parse each field by logical name. All required keys are assumed
        // present — the store layer enforces an all-or-nothing contract before
        // calling `from_raw`.
        let mem_version = raw
            .get("mem_version")
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CacheError::NotFound("missing key: mem_version".into()))?
            .parse()
            .map_err(|e| CacheError::ParseError(format!("invalid mem_version: {e}")))?;

        let acc_version = raw
            .get("acc_version")
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CacheError::NotFound("missing key: acc_version".into()))?
            .parse()
            .map_err(|e| CacheError::ParseError(format!("invalid acc_version: {e}")))?;

        let mem_active = raw
            .get("mem_active")
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CacheError::NotFound("missing key: mem_active".into()))?
            .parse()
            .map_err(|e| CacheError::ParseError(format!("invalid mem_active: {e}")))?;

        let acc_enabled = raw
            .get("acc_enabled")
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CacheError::NotFound("missing key: acc_enabled".into()))?
            .parse()
            .map_err(|e| CacheError::ParseError(format!("invalid acc_enabled: {e}")))?;

        let auth_scope = serde_json::from_str(
            raw.get("auth_scope")
                .and_then(|v| v.as_deref())
                .ok_or_else(|| CacheError::NotFound("missing key: auth_scope".into()))?,
        )
        .map_err(|e| CacheError::SerdeError(e))?;

        // The `sid` key is optional — tokens without sessions simply have none.
        let sid = raw
            .get("sid")
            .and_then(|v| v.as_deref())
            .map(Uuid::from_str)
            .transpose()
            .map_err(|e| CacheError::ParseError(format!("invalid sid: {e}")))?;

        Ok(Self {
            mem_id: Uuid::nil(),
            acc_id: Uuid::nil(),
            sid,
            mem_version,
            acc_version,
            mem_active,
            acc_enabled,
            auth_scope,
        })
    }
}

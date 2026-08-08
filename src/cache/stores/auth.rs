use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use tracing::{error, info};
use uuid::Uuid;

use crate::cache::{
    entities::auth::{AuthCache, AuthScopeCache},
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheExecutor, CacheKey},
};

pub struct AuthCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
    invalidation_success_count: AtomicU64,
    invalidation_failure_count: AtomicU64,
}

impl<C: CacheExecutor> AuthCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self {
            chx,
            invalidation_success_count: AtomicU64::new(0),
            invalidation_failure_count: AtomicU64::new(0),
        }
    }

    /// Pipeline-reads all keys for the entity. Returns None if any key is missing
    /// (all-or-nothing contract). Returns Some(hydrated_entity) if all keys present.
    pub async fn fetch(&self, keyed: &AuthCache) -> CacheResult<Option<AuthCache>> {
        let key_map = keyed.keys();
        let names: Vec<&str> = key_map.keys().map(|s| s.as_str()).collect();
        let key_strs: Vec<&str> = names.iter().map(|name| key_map[*name].as_ref()).collect();

        let values = self.chx.pipeline_get(&key_strs).await?;

        // All-or-nothing: any None means entity is invalid
        if values.iter().any(|v| v.is_none()) {
            return Ok(None);
        }

        let raw: HashMap<String, Option<String>> = names
            .into_iter()
            .zip(values)
            .map(|(name, val)| (name.to_string(), val))
            .collect();

        let entity = AuthCache::from_raw(raw)?;
        Ok(Some(entity))
    }

    /// Writes all keys for the entity with the given TTL (seconds).
    pub async fn write(&self, entity: &AuthCache, ttl: u64) -> CacheResult<()> {
        let key_map = entity.keys();
        for (name, cache_key) in &key_map {
            let val = match name.as_str() {
                "mem_version" => entity.mem_version.to_string(),
                "acc_version" => entity.acc_version.to_string(),
                "mem_active" => entity.mem_active.to_string(),
                "acc_enabled" => entity.acc_enabled.to_string(),
                "auth_scope" => serde_json::to_string(&entity.auth_scope)?,
                "sid" => entity.sid.map(|s| s.to_string()).unwrap_or_default(),
                _ => continue,
            };
            self.chx
                .set_string(cache_key.as_ref(), &val, Some(ttl))
                .await?;
        }
        info!(
            membership_id = %entity.mem_id,
            account_id = %entity.acc_id,
            "AUTH_CACHE_HYDRATED"
        );
        Ok(())
    }

    /// Deletes all keys for the entity. Used on invalidation.
    pub async fn invalidate(&self, entity: &AuthCache) -> CacheResult<()> {
        let key_map = entity.keys();
        let key_count = key_map.len();
        let result = async {
            for cache_key in key_map.values() {
                self.chx.del_key(cache_key.as_ref()).await?;
            }
            Ok::<_, CacheError>(())
        }
        .await;
        match &result {
            Ok(()) => {
                self.invalidation_success_count
                    .fetch_add(1, Ordering::Relaxed);
                info!(
                    membership_id = %entity.mem_id,
                    operation = "invalidate",
                    outcome = "success",
                    keys_deleted = key_count,
                    "AUTH_CACHE_INVALIDATED"
                );
            }
            Err(e) => {
                self.invalidation_failure_count
                    .fetch_add(1, Ordering::Relaxed);
                error!(
                    membership_id = %entity.mem_id,
                    operation = "invalidate",
                    outcome = "failure",
                    error = %e,
                    "AUTH_CACHE_INVALIDATION_FAILED"
                );
            }
        }
        result
    }

    pub async fn invalidate_account(&self, acc_id: &Uuid) -> CacheResult<()> {
        let acc_version_key = CacheKey::new("oxauth", "acc_v", acc_id);
        let acc_enabled_key = CacheKey::new("oxauth", "acc_en", acc_id);
        let result = async {
            self.chx.del_key(acc_version_key.as_ref()).await?;
            self.chx.del_key(acc_enabled_key.as_ref()).await?;
            Ok::<_, CacheError>(())
        }
        .await;
        match &result {
            Ok(()) => {
                self.invalidation_success_count
                    .fetch_add(1, Ordering::Relaxed);
                info!(
                    account_id = %acc_id,
                    operation = "invalidate_account",
                    outcome = "success",
                    keys_deleted = 2,
                    "AUTH_CACHE_ACCOUNT_INVALIDATED"
                );
            }
            Err(e) => {
                self.invalidation_failure_count
                    .fetch_add(1, Ordering::Relaxed);
                error!(
                    account_id = %acc_id,
                    operation = "invalidate_account",
                    outcome = "failure",
                    error = %e,
                    "AUTH_CACHE_ACCOUNT_INVALIDATION_FAILED"
                );
            }
        }
        result
    }

    pub async fn fetch_auth_scope(&self, mem_id: &Uuid) -> CacheResult<Option<AuthScopeCache>> {
        let auth_scope_key = CacheKey::new("oxauth", "auth_sc", mem_id);
        let values = self.chx.pipeline_get(&[auth_scope_key.as_ref()]).await?;
        match values.into_iter().next().flatten() {
            Some(raw) => match serde_json::from_str::<AuthScopeCache>(&raw) {
                Ok(scope) => Ok(Some(scope)),
                Err(_) => Ok(None),
            },
            None => Ok(None),
        }
    }

    pub fn invalidation_success_count(&self) -> u64 {
        self.invalidation_success_count.load(Ordering::Relaxed)
    }

    pub fn invalidation_failure_count(&self) -> u64 {
        self.invalidation_failure_count.load(Ordering::Relaxed)
    }
}

use std::{collections::HashMap, sync::Arc};

use tracing::info;

use crate::cache::{
    entities::auth::AuthCache,
    error::CacheResult,
    traits::{CacheEntity, CacheExecutor},
};

pub struct AuthCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> AuthCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
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
        for cache_key in key_map.values() {
            self.chx.del_key(cache_key.as_ref()).await?;
        }
        info!(membership_id = %entity.mem_id, "AUTH_CACHE_INVALIDATED");
        Ok(())
    }
}

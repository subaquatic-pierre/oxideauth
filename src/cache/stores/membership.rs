use std::{collections::HashMap, sync::Arc};

use crate::cache::{
    entities::membership::MembershipCache,
    error::CacheResult,
    traits::{CacheEntity, CacheExecutor},
};

pub struct MembershipCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> MembershipCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    /// Pipeline-reads the entity's key. Returns None if the key is missing
    /// (all-or-nothing contract). Returns Some(hydrated_entity) if present.
    pub async fn fetch(&self, keyed: &MembershipCache) -> CacheResult<Option<MembershipCache>> {
        let key_map = keyed.keys();
        let key_strs: Vec<&str> = key_map.values().map(|k| k.as_ref()).collect();
        let mut values = self.chx.pipeline_get(&key_strs).await?;
        if values.iter().any(|v| v.is_none()) {
            return Ok(None);
        }
        let raw: HashMap<String, Option<String>> = key_map
            .keys()
            .map(|s| s.as_str())
            .zip(values.drain(..))
            .map(|(name, val)| (name.to_string(), val))
            .collect();
        let entity = MembershipCache::from_raw(raw)?;
        Ok(Some(entity))
    }

    /// Writes the entity to Redis with the given TTL (seconds).
    pub async fn write(&self, entity: &MembershipCache, ttl: u64) -> CacheResult<()> {
        let key_map = entity.keys();
        let key_str = key_map.get("membership").unwrap().as_ref();
        let json_str = serde_json::to_string(entity)?;
        self.chx.set_string(key_str, &json_str, Some(ttl)).await?;
        Ok(())
    }

    /// Deletes the entity's key. Used on invalidation.
    pub async fn invalidate(&self, entity: &MembershipCache) -> CacheResult<()> {
        let key_map = entity.keys();
        for cache_key in key_map.values() {
            self.chx.del_key(cache_key.as_ref()).await?;
        }
        Ok(())
    }
}

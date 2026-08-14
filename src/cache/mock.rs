use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::cache::{
    error::CacheResult,
    traits::CacheExecutor,
};

/// In-memory cache executor for local development and tests.
///
/// Backed by a shared `HashMap<String, String>` so that reads observe prior
/// writes and deletes actually remove entries (unlike the previous no-op mock).
/// JSON values are stored as serialized strings, mirroring how they would be
/// stored in Redis.
///
/// The `path` and `ttl` parameters are accepted for `CacheExecutor` interface
/// compatibility but are ignored, matching the real Redis backend which also
/// ignores `path` for these commands.
#[derive(Debug, Default, Clone)]
pub struct MockChx {
    store: Arc<Mutex<HashMap<String, String>>>,
}

impl MockChx {
    /// Creates a new, empty in-memory cache.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CacheExecutor for MockChx {
    async fn json_get<T>(&self, key: &str, _path: Option<&str>) -> CacheResult<Option<T>>
    where
        T: DeserializeOwned + Send + Sync,
    {
        let raw = {
            let store = self
                .store
                .lock()
                .expect("MockChx store poisoned");
            store.get(key).cloned()
        };

        match raw {
            Some(json) => {
                let value: T = serde_json::from_str(&json)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    async fn json_set<T>(
        &self,
        key: &str,
        _path: Option<&str>,
        val: &T,
        _ttl: Option<u64>,
    ) -> CacheResult<()>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        let json = serde_json::to_string(val)?;
        self.store
            .lock()
            .expect("MockChx store poisoned")
            .insert(key.to_string(), json);
        Ok(())
    }

    async fn json_del<T>(&self, key: &str, _path: Option<&str>) -> CacheResult<u64>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        let removed = self
            .store
            .lock()
            .expect("MockChx store poisoned")
            .remove(key);
        Ok(if removed.is_some() { 1 } else { 0 })
    }

    async fn incr(&self, key: &str) -> CacheResult<i64> {
        let mut store = self.store.lock().expect("MockChx store poisoned");
        let current: i64 = store
            .get(key)
            .map(|v| v.parse().unwrap_or(0))
            .unwrap_or(0);
        let next = current + 1;
        store.insert(key.to_string(), next.to_string());
        Ok(next)
    }

    async fn set(&self, key: &str, val: &str, _ttl_seconds: Option<u64>) -> CacheResult<()> {
        self.store
            .lock()
            .expect("MockChx store poisoned")
            .insert(key.to_string(), val.to_string());
        Ok(())
    }

    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        Ok(self
            .store
            .lock()
            .expect("MockChx store poisoned")
            .get(key)
            .cloned())
    }

    async fn del(&self, key: &str) -> CacheResult<u64> {
        let removed = self
            .store
            .lock()
            .expect("MockChx store poisoned")
            .remove(key);
        Ok(if removed.is_some() { 1 } else { 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_cache_roundtrip() {
        let cache = MockChx::new();

        // plain string set/get
        assert_eq!(cache.get("k").await.unwrap(), None);
        cache.set("k", "v", None).await.unwrap();
        assert_eq!(cache.get("k").await.unwrap(), Some("v".to_string()));

        // json set/get
        cache.json_set("j", None, &42i32, None).await.unwrap();
        assert_eq!(cache.json_get::<i32>("j", None).await.unwrap(), Some(42));

        // incr (creates missing keys at 0, then increments)
        assert_eq!(cache.incr("n").await.unwrap(), 1);
        assert_eq!(cache.incr("n").await.unwrap(), 2);

        // del returns 1 when removed, 0 when absent
        assert_eq!(cache.del("k").await.unwrap(), 1);
        assert_eq!(cache.get("k").await.unwrap(), None);
        assert_eq!(cache.del("k").await.unwrap(), 0);

        // json_del removes the entry
        assert_eq!(cache.json_del::<i32>("j", None).await.unwrap(), 1);
        assert_eq!(cache.json_get::<i32>("j", None).await.unwrap(), None);
    }
}

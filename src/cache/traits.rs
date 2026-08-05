use std::sync::Arc;

use axum::async_trait;
use redis::{FromRedisValue, ToRedisArgs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::cache::error::CacheResult;

#[async_trait]
pub trait CacheExecutor: Send + Sync {
    async fn get<T>(&self, key: &str, path: Option<&str>) -> CacheResult<Option<T>>
    where
        T: DeserializeOwned + Send + Sync;

    async fn set<T>(
        &self,
        key: &str,
        path: Option<&str>,
        val: &T,
        ttl: Option<u64>,
    ) -> CacheResult<T>
    where
        T: DeserializeOwned + Serialize + Send + Sync;

    // Removes a key from the cache.
    async fn del<T>(&self, key: &str, path: Option<&str>) -> CacheResult<T>
    where
        T: DeserializeOwned + Serialize + Send + Sync;

    fn default_ttl(&self) -> u64;
}

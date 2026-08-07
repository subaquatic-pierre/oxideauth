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

    /// Fetches the raw string values for multiple keys in a single round trip.
    async fn pipeline_get(&self, keys: &[&str]) -> CacheResult<Vec<Option<String>>>;

    /// Stores a plain (non-JSON) string value with an optional TTL.
    ///
    /// Used for the auth-cache keys (`oxauth:tv:*`, `oxauth:sv:*`, ...) that are
    /// read back with `pipeline_get`, which issues plain Redis `GET` commands.
    async fn set_string(&self, key: &str, val: &str, ttl: Option<u64>) -> CacheResult<()>;

    /// Atomically increments the plain string value at `key` by one and returns
    /// the new value. Keys that do not exist are created with value `0` first.
    async fn incr(&self, key: &str) -> CacheResult<i64>;

    /// Deletes a plain (non-JSON) string key.
    async fn del_key(&self, key: &str) -> CacheResult<()>;
}

use std::{
    collections::HashMap,
    fmt::{self, Display, write},
    sync::Arc,
};

use axum::async_trait;
use redis::{FromRedisValue, ToRedisArgs};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::cache::error::CacheResult;

#[async_trait]
pub trait CacheExecutor: Send + Sync {
    async fn json_get<T>(&self, key: &str, path: Option<&str>) -> CacheResult<Option<T>>
    where
        T: DeserializeOwned + Send + Sync;

    async fn json_set<T>(
        &self,
        key: &str,
        path: Option<&str>,
        val: &T,
        ttl: Option<u64>,
    ) -> CacheResult<()>
    where
        T: DeserializeOwned + Serialize + Send + Sync;

    // Removes a key from the cache.
    async fn json_del<T>(&self, key: &str, path: Option<&str>) -> CacheResult<Option<T>>
    where
        T: DeserializeOwned + Serialize + Send + Sync;

    fn default_ttl(&self) -> u64;

    /// Fetches the raw string values for multiple keys in a single round trip.
    async fn pipeline_get(&self, keys: &[&str]) -> CacheResult<Vec<Option<String>>>;

    /// Atomically increments the plain string value at `key` by one and returns
    /// the new value. Keys that do not exist are created with value `0` first.
    async fn incr(&self, key: &str) -> CacheResult<i64>;

    async fn set(&self, key: &str, val: &str, ttl_seconds: Option<u64>) -> CacheResult<()>;
    async fn get(&self, key: &str) -> CacheResult<Option<String>>;
    async fn del(&self, key: &str) -> CacheResult<Option<String>>;
}

// ── CacheKey ────────────────────────────────────────────────────────

/// A typed cache key that formats itself as `{prefix}:{name}:{param}`.
///
/// The store layer never constructs or manipulates raw key strings — it
/// obtains them from the entity via `keys()` and passes them to Redis
/// commands through `AsRef<str>`.
///
/// # Example
///
/// ```rust,ignore
/// let key = CacheKey::new("oxauth", "mem_v", "550e8400-...");
/// assert_eq!(key.as_ref(), "oxauth:mem_v:550e8400-...");
/// ```
#[derive(Debug, Clone)]
pub struct CacheKey {
    key: String,
}

impl CacheKey {
    /// Builds the Redis key from its three components.
    pub fn new(prefix: &str, name: &str, param: impl fmt::Display) -> Self {
        Self {
            key: format!("{}:{}:{}", prefix, name, param),
        }
    }
}

impl Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key)
    }
}

impl AsRef<str> for CacheKey {
    fn as_ref(&self) -> &str {
        &self.key
    }
}

// ── CacheEntity ─────────────────────────────────────────────────────

/// Trait implemented by every typed cache entity.
///
/// The store layer uses `keys()` to discover which Redis keys belong to
/// an entity instance, pipelines them, and then calls `from_raw()` to
/// reconstruct the entity from the results.
///
/// # All-or-nothing contract
///
/// The store layer enforces that **every** declared key must be present
/// in Redis for the entity to be considered valid. If any key is missing,
/// `fetch` returns `None`. Implementations of `from_raw` MAY therefore
/// assume all required keys are present.
pub trait CacheEntity: Sized {
    /// Returns a named map of all cache keys for this entity instance.
    ///
    /// - Map **key**: logical name (e.g. `"mem_version"`, `"auth_scope"`)
    /// - Map **value**: the computed `CacheKey` wrapping the Redis key string
    fn _key() -> (&'static str, &'static str);
    fn key(&self) -> CacheKey;
    fn new_key(id: impl Display) -> CacheKey;
}

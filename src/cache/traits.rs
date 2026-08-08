use std::{collections::HashMap, fmt, sync::Arc};

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
    fn keys(&self) -> HashMap<String, CacheKey>;

    /// Parses the entity from raw pipeline results keyed by logical name.
    ///
    /// `raw` maps the logical name to the Redis string value (`None` if
    /// the key did not exist — but the store should have already rejected
    /// such cases before calling this method).
    fn from_raw(raw: HashMap<String, Option<String>>) -> CacheResult<Self>
    where
        Self: Sized;
}

use std::sync::Arc;

use axum::async_trait;
use redis::{
    AsyncCommands, Client, Commands, FromRedisValue, JsonAsyncCommands, SetOptions, ToRedisArgs,
    aio::{ConnectionManager, ConnectionManagerConfig, MultiplexedConnection},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;
use tracing::debug;

use crate::{
    cache::{
        error::{CacheError, CacheResult},
        traits::CacheExecutor,
    },
    core::error::CoreError,
};

pub struct RedisChx {
    client: Client,
    conn: Mutex<Option<MultiplexedConnection>>,
}

impl RedisChx {
    /// Creates a new RedisCacheExecutor. Takes a Redis connection string.
    pub async fn new(redis_url: &str) -> Self {
        debug!("Redis URL: {redis_url}");
        let client = Client::open(redis_url).expect("unable to create Redis Client");

        Self {
            client,
            conn: Mutex::new(None),
        }
    }
}

#[async_trait]
impl CacheExecutor for RedisChx {
    async fn conn(&self) -> CacheResult<MultiplexedConnection> {
        let mut guard = self.conn.lock().await;
        if let Some(c) = guard.as_ref() {
            return Ok(c.clone()); // cheap: Arc clone
        }
        let c = self.client.get_multiplexed_async_connection().await?;
        *guard = Some(c.clone());
        Ok(c)
    }

    /// Drop the cached connection so the next `conn()` re-establishes it.
    async fn invalidate_conn(&self) {
        *self.conn.lock().await = None;
    }

    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        let mut cmd = redis::cmd("GET");

        cmd.arg(key);

        let val = self.query_async(cmd).await?;

        Ok(val)
    }

    async fn set(&self, key: &str, val: &str, ttl_seconds: Option<u64>) -> CacheResult<()> {
        let str_val = serde_json::to_string(val)?;

        let mut cmd = redis::cmd("SET");

        cmd.arg(key).arg(&str_val);

        if let Some(ttl) = ttl_seconds {
            cmd.arg("EX").arg(ttl);
        }

        let val: String = self.query_async(cmd).await?;
        Ok(())
    }

    async fn del(&self, key: &str) -> CacheResult<u64> {
        let mut cmd = redis::cmd("DEL");

        let val: u64 = self.query_async(cmd).await?;

        Ok(val)
    }

    /// Retrieves a value from Redis and deserializes it from JSON.
    async fn json_get<T>(&self, key: &str, path: Option<&str>) -> CacheResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        let res = match self.get(key).await? {
            Some(val) => {
                let json = serde_json::from_str(&val)?;
                Some(json)
            }
            None => None,
        };

        Ok(res)
    }

    /// Serializes the value to JSON and stores it in Redis.
    async fn json_set<T>(
        &self,
        key: &str,
        path: Option<&str>,
        value: &T,
        ttl_seconds: Option<u64>,
    ) -> CacheResult<()>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        let str_val = serde_json::to_string(value)?;
        let res = self.set(key, &str_val, ttl_seconds).await?;

        Ok(())
    }

    /// Deletes a key from Redis.
    async fn json_del<T>(&self, key: &str, path: Option<&str>) -> CacheResult<u64>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        let count: u64 = self.del(key).await?;

        Ok(count)
    }

    /// Atomically increments the plain string value at `key` by one.
    async fn incr(&self, key: &str) -> CacheResult<u64> {
        let mut cmd = redis::cmd("INCR");
        cmd.arg(&key);

        let n: u64 = self.query_async(cmd).await?;
        Ok(n)
    }
}

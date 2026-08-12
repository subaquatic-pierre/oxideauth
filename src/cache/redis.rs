use axum::async_trait;
use redis::{
    AsyncCommands, Client, Commands, FromRedisValue, JsonAsyncCommands, SetOptions, ToRedisArgs,
    aio::{ConnectionManager, ConnectionManagerConfig},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
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
    conn: ConnectionManager,
    default_ttl: u64, // default ttl time is 30min in seconds
}

impl RedisChx {
    /// Creates a new RedisCacheExecutor. Takes a Redis connection string.
    pub async fn new(redis_url: &str) -> Self {
        debug!("Redis URL: {redis_url}");
        let client = Client::open(redis_url).expect("unable to create Redis Client");

        let conn = ConnectionManager::new(client.clone())
            .await
            .expect("unable to open connection");

        let default_ttl = 1800;

        Self {
            client,
            conn,
            default_ttl,
        }
    }
}

#[async_trait]
impl CacheExecutor for RedisChx {
    /// Retrieves a value from Redis and deserializes it from JSON.
    async fn json_get<T>(&self, key: &str, path: Option<&str>) -> CacheResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        // Get an asynchronous connection from the client
        let mut conn = self.conn.clone();

        let path = path.unwrap_or("$");

        let cached_value: String = conn.json_get(key, path).await?;
        let json: Vec<T> = serde_json::from_str(&cached_value)?;

        Ok(json.into_iter().next())
    }

    /// Serializes the value to JSON and stores it in Redis.
    async fn json_set<T>(
        &self,
        key: &str,
        path: Option<&str>,
        value: &T,
        ttl_seconds: Option<u64>,
    ) -> CacheResult<T>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        // Get an asynchronous connection from the client
        let mut conn = self.conn.clone();

        let path = path.unwrap_or("$");
        let str_val = serde_json::to_string(value)?;
        let ttl = ttl_seconds.unwrap_or(self.default_ttl());

        let mut cmd = redis::cmd("JSON.SET");

        cmd.arg(key) // 1st argument: key
            .arg(path) // 2nd argument: path
            .arg(&str_val) // 3rd argument: value (serialized JSON string)
            .arg("EX") // 4th argument: TTL option
            .arg(ttl); // 5th argument: TTL value

        let updated: String = cmd.query_async(&mut conn).await?;

        let json: Vec<T> = serde_json::from_str(&updated)?;

        let ret = json
            .into_iter()
            .next()
            .ok_or(CacheError::InvalidSetOperation(
                format!(
                    "unable to set key: {}, at path: {} for value {}",
                    key, path, str_val
                )
                .to_string(),
            ))?;

        Ok(ret)
    }

    /// Deletes a key from Redis.
    async fn json_del<T>(&self, key: &str, path: Option<&str>) -> CacheResult<T>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        let mut conn = self.conn.clone();
        let path = path.unwrap_or("$");

        let deleted: String = conn.json_del(key, path).await?;

        let json: Vec<T> = serde_json::from_str(&deleted)?;

        let ret = json
            .into_iter()
            .next()
            .ok_or(CacheError::InvalidSetOperation(
                format!("unable to delete key: {} at path: {}", key, path).to_string(),
            ))?;

        Ok(ret)
    }

    fn default_ttl(&self) -> u64 {
        self.default_ttl
    }

    /// Fetches the raw string values for multiple keys in a single round trip.
    async fn pipeline_get(&self, keys: &[&str]) -> CacheResult<Vec<Option<String>>> {
        let mut pipe = redis::pipe();
        pipe.mget(keys);
        let (res,) = pipe
            .query_async::<(Vec<Option<String>>,)>(&mut self.conn.clone())
            .await?;
        Ok(res)
    }

    /// Atomically increments the plain string value at `key` by one.
    async fn incr(&self, key: &str) -> CacheResult<i64> {
        let mut conn = self.conn.clone();
        let n = conn.incr(key, 1).await?;
        Ok(n)
    }

    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        let mut conn = self.conn.clone();
        let val = conn.get(key).await?;

        Ok(val)
    }

    async fn set(&self, key: &str, val: &str, ttl_seconds: Option<u64>) -> CacheResult<String> {
        let ttl = ttl_seconds.unwrap_or(self.default_ttl());

        let mut conn = self.conn.clone();
        let val = conn.set(key, ttl).await?;

        Ok(val)
    }

    async fn del(&self, key: &str) -> CacheResult<Option<String>> {
        let mut conn = self.conn.clone();
        let val = conn.del(key).await?;

        Ok(val)
    }
}

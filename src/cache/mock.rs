use axum::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::cache::{
    error::{CacheError, CacheResult},
    traits::CacheExecutor,
};

/// Mock cache executor for local development and tests.
/// Every operation is a no-op: reads return `None`, writes return the input.
#[derive(Debug, Default, Clone)]
pub struct MockChx {}

#[async_trait]
impl CacheExecutor for MockChx {
    async fn json_get<T>(&self, _key: &str, _path: Option<&str>) -> CacheResult<Option<T>>
    where
        T: DeserializeOwned + Send + Sync,
    {
        Ok(None)
    }

    async fn json_set<T>(
        &self,
        _key: &str,
        _path: Option<&str>,
        val: &T,
        _ttl: Option<u64>,
    ) -> CacheResult<T>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        // Round-trip through JSON so we can return a value of type `T`.
        let serialized = serde_json::to_string(val)?;
        let ret = serde_json::from_str::<T>(&serialized)?;
        Ok(ret)
    }

    async fn json_del<T>(&self, _key: &str, _path: Option<&str>) -> CacheResult<T>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        Err(CacheError::NotFound(
            "mock cache: key not found".to_string(),
        ))
    }

    fn default_ttl(&self) -> u64 {
        0
    }

    async fn pipeline_get(&self, keys: &[&str]) -> CacheResult<Vec<Option<String>>> {
        Ok(vec![None; keys.len()])
    }

    async fn incr(&self, _key: &str) -> CacheResult<i64> {
        Ok(0)
    }

    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        Ok(Some("".to_string()))
    }
    async fn set(&self, key: &str, val: &str, _ttl: Option<u64>) -> CacheResult<String> {
        Ok("".to_string())
    }
    async fn del(&self, key: &str) -> CacheResult<Option<String>> {
        Ok(Some("".to_string()))
    }
}

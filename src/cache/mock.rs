use axum::async_trait;
use redis::aio::MultiplexedConnection;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    cache::{
        error::{CacheError, CacheResult},
        traits::CacheExecutor,
    },
    core::error::CoreResult,
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
    ) -> CacheResult<()>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        Ok(())
    }

    async fn json_del<T>(&self, _key: &str, _path: Option<&str>) -> CacheResult<u64>
    where
        T: DeserializeOwned + Serialize + Send + Sync,
    {
        Ok(42)
    }

    async fn incr(&self, _key: &str) -> CacheResult<i64> {
        Ok(0)
    }

    async fn get(&self, key: &str) -> CacheResult<Option<String>> {
        Ok(Some("".to_string()))
    }

    async fn set(&self, key: &str, val: &str, _ttl: Option<u64>) -> CacheResult<()> {
        Ok(())
    }

    async fn del(&self, key: &str) -> CacheResult<u64> {
        Ok(0)
    }
}

use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::{
    cache::{error::CacheResult, traits::CacheExecutor},
    core::models::membership::CachedMembership,
};

pub struct MembershipCache<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> MembershipCache<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    pub async fn get(&self, id: Uuid, path: Option<&str>) -> CacheResult<Option<CachedMembership>> {
        let res = self
            .chx
            .get::<CachedMembership>(&id.to_string(), path)
            .await?;

        Ok(res)
    }

    pub async fn set(
        &self,
        id: Uuid,
        path: Option<&str>,
        value: &CachedMembership,
        ttl: Option<u64>,
    ) -> CacheResult<CachedMembership> {
        let res = self.chx.set(&id.to_string(), path, value, ttl).await?;

        Ok(res)
    }

    pub async fn clear(&self, id: Uuid, path: Option<&str>) -> CacheResult<CachedMembership> {
        let res = self.chx.del(&id.to_string(), path).await?;

        Ok(res)
    }

    // clear CachedMembership
}

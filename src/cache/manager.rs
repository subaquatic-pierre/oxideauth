use std::sync::Arc;

use crate::cache::{stores::membership::MembershipCache, traits::CacheExecutor};

pub struct CacheManager<C: CacheExecutor> {
    chx: Arc<C>,
    pub membership: MembershipCache<C>,
}

impl<C: CacheExecutor> CacheManager<C> {
    pub fn new(chx: Arc<C>) -> Self {
        let membership = MembershipCache::new(chx.clone());

        Self { chx, membership }
    }

    /// Returns a clone of the underlying cache executor.
    ///
    /// This is used by services (e.g. `AuthService` for Redis-backed rate
    /// limiting) that need generic key/value access to the cache layer.
    pub fn executor(&self) -> Arc<C> {
        self.chx.clone()
    }
}

use std::sync::Arc;

use crate::cache::{
    stores::{auth::AuthCacheStore, membership::MembershipCacheStore},
    traits::CacheExecutor,
};

pub struct CacheManager<C: CacheExecutor> {
    chx: Arc<C>,
    pub membership: MembershipCacheStore<C>,
    pub auth: AuthCacheStore<C>,
}

impl<C: CacheExecutor> CacheManager<C> {
    pub fn new(chx: Arc<C>) -> Self {
        let membership = MembershipCacheStore::new(chx.clone());
        let auth = AuthCacheStore::new(chx.clone());

        Self { chx, membership, auth }
    }

    /// Returns a clone of the underlying cache executor.
    ///
    /// This is used by services (e.g. `AuthService` for Redis-backed rate
    /// limiting) that need generic key/value access to the cache layer.
    pub fn executor(&self) -> Arc<C> {
        self.chx.clone()
    }
}

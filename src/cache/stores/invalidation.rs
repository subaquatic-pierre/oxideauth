use std::sync::Arc;
use uuid::Uuid;

use crate::cache::{
    entities::auth::AuthCache,
    error::CacheResult,
    traits::CacheExecutor,
    stores::auth::AuthCacheStore,
};

/// Thin wrapper around `AuthCacheStore` that accepts raw identifiers
/// instead of requiring callers to construct `AuthCache::new_keyed()`.
pub struct AuthCacheInvalidationService<C: CacheExecutor> {
    auth_store: Arc<AuthCacheStore<C>>,
}

impl<C: CacheExecutor> AuthCacheInvalidationService<C> {
    pub fn new(auth_store: Arc<AuthCacheStore<C>>) -> Self {
        Self { auth_store }
    }

    /// Invalidates all auth cache entries for the given membership and account.
    ///
    /// Internally constructs `AuthCache::new_keyed(mem_id, acc_id, sid)` and
    /// delegates to `AuthCacheStore::invalidate`.
    pub async fn invalidate(
        &self,
        mem_id: Uuid,
        acc_id: Uuid,
        sid: Option<Uuid>,
    ) -> CacheResult<()> {
        let keyed = AuthCache::new_keyed(mem_id, acc_id, sid);
        self.auth_store.invalidate(&keyed).await
    }
}

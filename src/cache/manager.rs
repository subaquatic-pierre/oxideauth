use std::sync::Arc;

use crate::cache::{
    stores::{
        auth::AuthCacheStore,
        invalidation::AuthCacheInvalidationService,
        membership::MembershipCacheStore,
        oauth_state::OAuthStateCacheStore,
        replay::RefreshTokenReplayCacheStore,
    },
    traits::CacheExecutor,
};

pub struct CacheManager<C: CacheExecutor> {
    chx: Arc<C>,
    pub membership: MembershipCacheStore<C>,
    pub auth: Arc<AuthCacheStore<C>>,
    pub replay: RefreshTokenReplayCacheStore<C>,
    pub oauth_state: OAuthStateCacheStore<C>,
    pub invalidation: AuthCacheInvalidationService<C>,
}

impl<C: CacheExecutor> CacheManager<C> {
    pub fn new(chx: Arc<C>) -> Self {
        let membership = MembershipCacheStore::new(chx.clone());
        let auth = Arc::new(AuthCacheStore::new(chx.clone()));
        let replay = RefreshTokenReplayCacheStore::new(chx.clone());
        let oauth_state = OAuthStateCacheStore::new(chx.clone());
        let invalidation = AuthCacheInvalidationService::new(auth.clone());

        Self {
            chx,
            membership,
            auth,
            replay,
            oauth_state,
            invalidation,
        }
    }

    /// Returns a clone of the underlying cache executor.
    ///
    /// This is used by services (e.g. `AuthService` for Redis-backed rate
    /// limiting) that need generic key/value access to the cache layer.
    pub fn executor(&self) -> Arc<C> {
        self.chx.clone()
    }
}

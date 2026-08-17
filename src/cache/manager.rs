use std::sync::Arc;

use crate::cache::{
    stores::{
        auth::AuthCacheStore, client_auth::ClientAuthCacheStore, oauth_state::OAuthStateCacheStore,
        policy::PolicyCacheStore, replay::RefreshTokenReplayCacheStore, workspace::WorkspaceCacheStore,
    },
    traits::CacheExecutor,
};

pub struct CacheManager<C: CacheExecutor> {
    chx: Arc<C>,
    pub auth: Arc<AuthCacheStore<C>>,
    pub client_auth: ClientAuthCacheStore<C>,
    pub replay: RefreshTokenReplayCacheStore<C>,
    pub oauth_state: OAuthStateCacheStore<C>,
    pub workspace: WorkspaceCacheStore<C>,
    pub policy: PolicyCacheStore<C>,
}

impl<C: CacheExecutor> CacheManager<C> {
    pub fn new(chx: Arc<C>) -> Self {
        let auth = Arc::new(AuthCacheStore::new(chx.clone()));
        let client_auth = ClientAuthCacheStore::new(chx.clone());
        let replay = RefreshTokenReplayCacheStore::new(chx.clone());
        let oauth_state = OAuthStateCacheStore::new(chx.clone());
        let workspace = WorkspaceCacheStore::new(chx.clone());
        let policy = PolicyCacheStore::new(chx.clone());

        Self {
            chx,
            auth,
            client_auth,
            replay,
            oauth_state,
            workspace,
            policy,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{
        entities::auth::AuthCache,
        mock::MockChx,
        traits::CacheEntity,
    };
    use uuid::Uuid;

    #[test]
    fn test_new_constructs_all_sub_stores_and_returns_same_executor() {
        let chx = Arc::new(MockChx::new());
        let mgr = CacheManager::new(chx.clone());

        // The executor returned by `executor()` is the same Arc passed to `new`.
        assert!(Arc::ptr_eq(&mgr.executor(), &chx));

        // All sub-stores are constructed and typed correctly.
        let _ = &mgr.auth;
        let _ = &mgr.replay;
        let _ = &mgr.oauth_state;
        let _ = &mgr.workspace;
        let _ = &mgr.policy;
    }

    #[tokio::test]
    async fn test_sub_stores_share_the_same_executor() {
        let chx = Arc::new(MockChx::new());
        let mgr = CacheManager::new(chx.clone());

        // A write through the auth sub-store is visible through `executor()`.
        let mem = Uuid::new_v4();
        let entity = AuthCache::new_keyed(mem, Uuid::new_v4(), None);
        mgr.auth.write(&entity, None).await.unwrap();

        let raw = mgr
            .executor()
            .json_get::<AuthCache>(entity.key().as_ref(), None)
            .await
            .unwrap();
        assert!(raw.is_some(), "write via sub-store should be visible");
        assert_eq!(raw.unwrap().mem_id, mem);
    }
}

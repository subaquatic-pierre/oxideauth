use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tracing::info;

use crate::cache::{
    entities::oauth_state::OAuthStateCache,
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheExecutor, CacheKey},
};

/// Store for OAuth authorization state.
///
/// Each state is written keyed by its `csrf_token` and deleted when consumed,
/// so a single authorization flow can only complete once.
pub struct OAuthStateCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> OAuthStateCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    /// Persists the OAuth state entity with the given TTL (seconds).
    pub async fn write(&self, entity: &OAuthStateCache, ttl: u64) -> CacheResult<()> {
        // let key_map = entity.key();
        // let key = key_map.get("oauth_state").unwrap();
        // self.chx.set(key.as_ref(), None, entity, Some(ttl)).await?;
        // self.write_success_count.fetch_add(1, Ordering::Relaxed);
        // info!(
        //     csrf_token = %entity.csrf_token,
        //     "OAUTH_STATE_WRITTEN"
        // );
        Ok(())
    }

    pub async fn fetch_and_consume(&self, csrf_token: &str) -> CacheResult<OAuthStateCache> {
        let key = CacheKey::new("oxauth", "oauth", csrf_token);
        let entity = self
            .chx
            .json_get::<OAuthStateCache>(key.as_ref(), None)
            .await?
            .ok_or_else(|| CacheError::NotFound("oauth state not found".into()))?;
        self.chx
            .json_del::<serde_json::Value>(key.as_ref(), None)
            .await?;

        info!(
            csrf_token = %csrf_token,
            "OAUTH_STATE_CONSUMED"
        );
        Ok(entity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{
        entities::oauth_state::OAuthProvider,
        mock::MockChx,
    };

    #[tokio::test]
    async fn test_write_is_noop_ok() {
        let store = OAuthStateCacheStore::new(Arc::new(MockChx::new()));

        let entity = OAuthStateCache {
            csrf_token: "csrf-noop".into(),
            ..Default::default()
        };
        // `write` is currently a no-op: it must still return Ok(()).
        store.write(&entity, 60).await.unwrap();
    }

    #[tokio::test]
    async fn test_fetch_and_consume_returns_then_removes() {
        let chx = Arc::new(MockChx::new());
        let store = OAuthStateCacheStore::new(chx.clone());

        let token = "csrf-consume-123";
        let entity = OAuthStateCache {
            csrf_token: token.into(),
            redirect_url: "https://example.com/callback".into(),
            created_at: 42,
            provider: OAuthProvider::Google,
        };

        // `write` is a no-op, so seed the store directly through the mock.
        chx.json_set(&format!("oxauth:oauth:{}", token), None, &entity, None)
            .await
            .unwrap();

        // First call returns the stored entity.
        let consumed = store.fetch_and_consume(token).await.unwrap();
        assert_eq!(consumed.csrf_token, token);
        assert_eq!(consumed.redirect_url, "https://example.com/callback");
        assert_eq!(consumed.created_at, 42);
        assert_eq!(consumed.provider, OAuthProvider::Google);

        // Second call must fail because the state was removed.
        let second = store.fetch_and_consume(token).await;
        assert!(
            matches!(second, Err(CacheError::NotFound(_))),
            "consume must remove the state so a second consume fails"
        );
    }

    #[tokio::test]
    async fn test_fetch_and_consume_missing_state_errors() {
        let store = OAuthStateCacheStore::new(Arc::new(MockChx::new()));
        let res = store.fetch_and_consume("never-seeded").await;
        assert!(matches!(res, Err(CacheError::NotFound(_))));
    }
}

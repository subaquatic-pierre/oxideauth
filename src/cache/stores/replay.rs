use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tracing::info;
use uuid::Uuid;

use crate::cache::{
    entities::replay::RefreshTokenReplayCache,
    error::CacheResult,
    traits::{CacheExecutor, CacheKey},
};

/// Store for refresh-token replay protection.
///
/// Each refresh token `jti` is marked as consumed (with the session id that
/// used it) the first time it is seen. Any subsequent use of the same `jti` is
/// detected as a replay attempt.
pub struct RefreshTokenReplayCacheStore<C: CacheExecutor> {
    chx: Arc<C>,
}

impl<C: CacheExecutor> RefreshTokenReplayCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self { chx }
    }

    /// Checks whether the refresh token with the given `jti` has already been
    /// consumed.
    ///
    /// - `Ok(true)`  — the token was already consumed: a replay was detected.
    /// - `Ok(false)` — the token was fresh: it has now been marked as consumed
    ///   with the given `sid` and TTL (seconds).
    pub async fn check_and_consume(&self, jti: Uuid, sid: Uuid, ttl: i64) -> CacheResult<bool> {
        let key = CacheKey::new("oxauth", "crt", jti);
        if self
            .chx
            .json_get::<RefreshTokenReplayCache>(key.as_ref(), None)
            .await?
            .is_some()
        {
            info!(
                jti = %jti,
                "REFRESH_TOKEN_REPLAY_DETECTED"
            );
            return Ok(true);
        }

        let val = RefreshTokenReplayCache {
            jti,
            sid: Some(sid),
        };

        self.chx
            .json_set(key.as_ref(), None, &val, Some(ttl))
            .await?;
        info!(
            jti = %jti,
            sid = %sid,
            "REFRESH_TOKEN_CONSUMED"
        );
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::mock::MockChx;

    #[tokio::test]
    async fn test_check_and_consume_marks_then_detects_replay() {
        let store = RefreshTokenReplayCacheStore::new(Arc::new(MockChx::new()));

        let jti = Uuid::new_v4();
        let sid = Uuid::new_v4();

        // First use: fresh token, no replay.
        let first = store.check_and_consume(jti, sid, 60).await.unwrap();
        assert!(!first, "first use of a fresh jti must not be a replay");

        // Second use with the same jti: replay detected.
        let second = store.check_and_consume(jti, sid, 60).await.unwrap();
        assert!(second, "second use of the same jti must be a replay");
    }

    #[tokio::test]
    async fn test_check_and_consume_is_keyed_per_jti() {
        let store = RefreshTokenReplayCacheStore::new(Arc::new(MockChx::new()));

        let jti_a = Uuid::new_v4();
        let jti_b = Uuid::new_v4();
        let sid = Uuid::new_v4();

        let first = store.check_and_consume(jti_a, sid, 60).await.unwrap();
        assert!(!first);

        // A different jti is still fresh.
        let other = store.check_and_consume(jti_b, sid, 60).await.unwrap();
        assert!(!other, "a different jti must not be flagged as replay");

        // But jti_a is now consumed.
        let replay = store.check_and_consume(jti_a, sid, 60).await.unwrap();
        assert!(replay);
    }
}

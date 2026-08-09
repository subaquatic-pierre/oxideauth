use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
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
    replay_detected_count: AtomicU64,
    token_consumed_count: AtomicU64,
}

impl<C: CacheExecutor> RefreshTokenReplayCacheStore<C> {
    pub fn new(chx: Arc<C>) -> Self {
        Self {
            chx,
            replay_detected_count: AtomicU64::new(0),
            token_consumed_count: AtomicU64::new(0),
        }
    }

    /// Checks whether the refresh token with the given `jti` has already been
    /// consumed.
    ///
    /// - `Ok(true)`  — the token was already consumed: a replay was detected.
    /// - `Ok(false)` — the token was fresh: it has now been marked as consumed
    ///   with the given `sid` and TTL (seconds).
    pub async fn check_and_consume(&self, jti: Uuid, sid: Uuid, ttl: u64) -> CacheResult<bool> {
        let key = CacheKey::new("oxauth", "crt", jti);
        if self.chx.get::<String>(key.as_ref(), None).await?.is_some() {
            self.replay_detected_count.fetch_add(1, Ordering::Relaxed);
            info!(
                jti = %jti,
                "REFRESH_TOKEN_REPLAY_DETECTED"
            );
            return Ok(true);
        }

        self.chx
            .set_string(key.as_ref(), &sid.to_string(), Some(ttl))
            .await?;
        self.token_consumed_count.fetch_add(1, Ordering::Relaxed);
        info!(
            jti = %jti,
            sid = %sid,
            "REFRESH_TOKEN_CONSUMED"
        );
        Ok(false)
    }

    pub fn replay_detected_count(&self) -> u64 {
        self.replay_detected_count.load(Ordering::Relaxed)
    }

    pub fn token_consumed_count(&self) -> u64 {
        self.token_consumed_count.load(Ordering::Relaxed)
    }
}

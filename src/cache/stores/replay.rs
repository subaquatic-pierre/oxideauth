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
    pub async fn check_and_consume(&self, jti: Uuid, sid: Uuid, ttl: u64) -> CacheResult<bool> {
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

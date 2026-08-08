use std::sync::Arc;

use crate::{
    cache::{
        entities::client::ClientRateLimitState,
        error::CacheResult,
        traits::CacheExecutor,
    },
    utils::time::now_utc,
};

/// Key prefix for client validation rate limit counters.
const CLIENT_RATE_LIMIT_KEY_PREFIX: &str = "oxauth:cl:ratelimit:";

/// Key prefix for successful push notification counters.
const CLIENT_PUSH_OK_KEY_PREFIX: &str = "oxauth:cl:push:ok:";

/// Key prefix for failed push notification counters.
const CLIENT_PUSH_FAIL_KEY_PREFIX: &str = "oxauth:cl:push:fail:";

/// Sliding-window rate limiter for client validation attempts.
///
/// State is persisted in Redis through the generic [`CacheExecutor`], following
/// the exact pattern of `AuthService::check_rate_limit` (state struct stored
/// as JSON with a TTL equal to the window length).
pub struct ClientRateLimitCache<C: CacheExecutor> {
    executor: Arc<C>,
    max_attempts: u32,
    window_secs: i64,
}

impl<C: CacheExecutor> ClientRateLimitCache<C> {
    pub fn new(executor: Arc<C>, max_attempts: u32, window_secs: i64) -> Self {
        Self {
            executor,
            max_attempts,
            window_secs,
        }
    }

    /// Enforces a sliding window rate limit for the given client.
    ///
    /// Key: `oxauth:cl:ratelimit:{client_id}`
    ///
    /// Returns `Ok(true)` if the attempt is allowed (the counter was
    /// incremented), `Ok(false)` if the client has exceeded `max_attempts`
    /// within `window_secs`.
    pub async fn check_rate_limit(&self, client_id: &str) -> CacheResult<bool> {
        let cache_key = format!("{}{}", CLIENT_RATE_LIMIT_KEY_PREFIX, client_id);
        let now = now_utc().unix_timestamp();

        let mut state = self
            .executor
            .get::<ClientRateLimitState>(&cache_key, None)
            .await?
            .unwrap_or(ClientRateLimitState::new(0, now));

        // Reset the window if it has elapsed.
        if now - state.window_start >= self.window_secs as i64 {
            state = ClientRateLimitState::new(0, now);
        }

        if state.count >= self.max_attempts {
            return Ok(false);
        }

        state.count += 1;
        self.executor
            .set(&cache_key, None, &state, Some(self.window_secs as u64))
            .await?;

        Ok(true)
    }

    /// Clears the rate limit counter for the given client (called on success).
    pub async fn reset_rate_limit(&self, client_id: &str) -> CacheResult<()> {
        let cache_key = format!("{}{}", CLIENT_RATE_LIMIT_KEY_PREFIX, client_id);
        self.executor
            .del::<ClientRateLimitState>(&cache_key, None)
            .await?;
        Ok(())
    }

    /// Increments the successful-push counter for the given client.
    ///
    /// Key: `oxauth:cl:push:ok:{client_id}`
    pub async fn increment_push_ok(&self, client_id: &str) -> CacheResult<()> {
        let cache_key = format!("{}{}", CLIENT_PUSH_OK_KEY_PREFIX, client_id);
        self.executor.incr(&cache_key).await?;
        Ok(())
    }

    /// Increments the failed-push counter for the given client.
    ///
    /// Key: `oxauth:cl:push:fail:{client_id}`
    pub async fn increment_push_fail(&self, client_id: &str) -> CacheResult<()> {
        let cache_key = format!("{}{}", CLIENT_PUSH_FAIL_KEY_PREFIX, client_id);
        self.executor.incr(&cache_key).await?;
        Ok(())
    }
}

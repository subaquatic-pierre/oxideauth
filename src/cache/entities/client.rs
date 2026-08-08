use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cache::{
    error::CacheResult,
    traits::{CacheEntity, CacheKey},
};

/// Small state object persisted in Redis to track client validation rate
/// limiting counters. Mirrors the `RateLimitState` used by `AuthService`.
///
/// The rate-limit counter (`oxauth:cl:ratelimit:{client_id}`) is stored as a
/// JSON-serialized instance of this struct, while the push-notification
/// counters are plain incr counters under `oxauth:cl:push:ok:{client_id}` and
/// `oxauth:cl:push:fail:{client_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRateLimitState {
    pub count: u32,
    pub window_start: i64,
    pub client_id: String,
    pub push_ok: i64,
    pub push_fail: i64,
}

impl ClientRateLimitState {
    /// Builds a fresh, zeroed state for the given client.
    pub fn new_keyed(client_id: String) -> Self {
        Self {
            count: 0,
            window_start: 0,
            client_id,
            push_ok: 0,
            push_fail: 0,
        }
    }

    /// Builds a rate-limit-only state with the given counter and window start.
    ///
    /// Used by [`ClientRateLimitCache`](crate::cache::stores::client::ClientRateLimitCache)
    /// to reset a window without touching the push counters.
    pub fn new(count: u32, window_start: i64) -> Self {
        Self {
            count,
            window_start,
            client_id: String::new(),
            push_ok: 0,
            push_fail: 0,
        }
    }
}

impl CacheEntity for ClientRateLimitState {
    fn keys(&self) -> HashMap<String, CacheKey> {
        let mut map = HashMap::new();
        map.insert(
            "ratelimit".into(),
            CacheKey::new("oxauth", "cl:ratelimit", &self.client_id),
        );
        map.insert(
            "push_ok".into(),
            CacheKey::new("oxauth", "cl:push:ok", &self.client_id),
        );
        map.insert(
            "push_fail".into(),
            CacheKey::new("oxauth", "cl:push:fail", &self.client_id),
        );
        map
    }

    fn from_raw(raw: HashMap<String, Option<String>>) -> CacheResult<Self> {
        // Parse each field from the raw map. The rate-limit key stores a JSON
        // `ClientRateLimitState` while the push counters are plain incr
        // counters, so numeric parses fall back to `0` when the value is not a
        // plain number. `client_id` is not stored in cache values.
        let count = raw
            .get("ratelimit")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let window_start = raw
            .get("ratelimit")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let push_ok = raw
            .get("push_ok")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let push_fail = raw
            .get("push_fail")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Ok(Self {
            count,
            window_start,
            client_id: String::new(),
            push_ok,
            push_fail,
        })
    }
}

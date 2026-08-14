use std::str::FromStr;
use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cache::{
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheKey},
};

/// The refresh-token replay cache entity persisted under `oxauth:crt:{jti}`.
///
/// A single plain-string value records the session id (`sid`) that consumed the
/// refresh token. Once present, the token can never be replayed again.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefreshTokenReplayCache {
    pub jti: Uuid,
    pub sid: Option<Uuid>,
}

impl RefreshTokenReplayCache {
    pub fn new(jti: Uuid) -> Self {
        Self { jti, sid: None }
    }
}

impl CacheEntity for RefreshTokenReplayCache {
    fn _key() -> (&'static str, &'static str) {
        ("oxauth", "crt")
    }

    fn key(&self) -> CacheKey {
        let (prefix, name) = Self::_key();
        CacheKey::new(prefix, name, self.jti)
    }

    fn new_key(jti: impl Display) -> CacheKey {
        let (prefix, name) = Self::_key();
        CacheKey::new(prefix, name, jti)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_sets_sid_none() {
        let jti = Uuid::new_v4();
        let replay = RefreshTokenReplayCache::new(jti);
        assert_eq!(replay.jti, jti);
        assert_eq!(replay.sid, None);
    }

    #[test]
    fn test_default() {
        let replay = RefreshTokenReplayCache::default();
        assert_eq!(replay.jti, Uuid::nil());
        assert_eq!(replay.sid, None);
    }

    #[test]
    fn test_key_format() {
        let jti = Uuid::new_v4();
        let replay = RefreshTokenReplayCache::new(jti);

        assert_eq!(replay.key().as_ref(), format!("oxauth:crt:{}", jti));
        assert_eq!(RefreshTokenReplayCache::new_key(jti).as_ref(), format!("oxauth:crt:{}", jti));
        assert_eq!(RefreshTokenReplayCache::_key(), ("oxauth", "crt"));
    }
}

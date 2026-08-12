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

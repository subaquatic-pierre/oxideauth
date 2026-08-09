use std::collections::HashMap;
use std::str::FromStr;

use uuid::Uuid;

use crate::cache::{
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheKey},
};

/// The refresh-token replay cache entity persisted under `oxauth:crt:{jti}`.
///
/// A single plain-string value records the session id (`sid`) that consumed the
/// refresh token. Once present, the token can never be replayed again.
#[derive(Debug, Clone, Default)]
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
    fn keys(&self) -> HashMap<String, CacheKey> {
        let mut map = HashMap::new();
        map.insert(
            "consumed".into(),
            CacheKey::new("oxauth", "crt", self.jti),
        );
        map
    }

    fn from_raw(raw: HashMap<String, Option<String>>) -> CacheResult<Self> {
        // The value is a plain string holding the session id that consumed the
        // token. Empty or missing values mean the token has not been consumed.
        match raw.get("consumed").and_then(|v| v.as_deref()) {
            Some(raw_sid) if !raw_sid.is_empty() => {
                let sid = Uuid::from_str(raw_sid)
                    .map_err(|e| CacheError::ParseError(format!("invalid sid: {e}")))?;
                Ok(Self {
                    jti: Uuid::nil(),
                    sid: Some(sid),
                })
            }
            _ => Ok(Self::default()),
        }
    }
}

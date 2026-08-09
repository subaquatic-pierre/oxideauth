use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cache::{
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheKey},
};

/// The OAuth provider an authorization request was started with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OAuthProvider {
    Google,
}

impl OAuthProvider {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Google => "google",
        }
    }
}

impl Default for OAuthProvider {
    fn default() -> Self {
        Self::Google
    }
}

/// The OAuth state entity persisted under `oxauth:oauth:{csrf_token}`.
///
/// It carries everything needed to resume an OAuth authorization flow after the
/// provider redirects the browser back to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthStateCache {
    pub csrf_token: String,
    pub redirect_url: String,
    pub created_at: i64,
    pub provider: OAuthProvider,
}

impl Default for OAuthStateCache {
    fn default() -> Self {
        Self {
            csrf_token: String::new(),
            redirect_url: String::new(),
            created_at: 0,
            provider: OAuthProvider::default(),
        }
    }
}

impl CacheEntity for OAuthStateCache {
    fn keys(&self) -> HashMap<String, CacheKey> {
        let mut map = HashMap::new();
        map.insert(
            "oauth_state".into(),
            CacheKey::new("oxauth", "oauth", &self.csrf_token),
        );
        map
    }

    fn from_raw(raw: HashMap<String, Option<String>>) -> CacheResult<Self> {
        let json_str = raw
            .get("oauth_state")
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CacheError::NotFound("missing key: oauth_state".into()))?;
        serde_json::from_str(json_str).map_err(CacheError::SerdeError)
    }
}

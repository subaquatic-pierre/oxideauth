use std::fmt::Display;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

pub use crate::store::entities::credential::CredentialStatus;
use crate::cache::traits::{CacheEntity, CacheKey};

/// The cached payload for credential-based (API-key) client authentication.
///
/// Persisted under `oxauth:client_id:{credential_id}`. It carries everything
/// needed to authorize an API client without hitting the database on every
/// request. Hydration happens in the service layer via
/// [`crate::cache::entities::auth::AuthCache::build_from_db`], which resolves
/// the membership -> roles -> permissions graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientAuthCache {
    pub credential_id: Uuid,
    pub membership_id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub roles: Vec<Uuid>,
    pub permissions: Vec<String>,
    pub expires_at: Option<OffsetDateTime>,
    pub status: CredentialStatus,
}

impl ClientAuthCache {
    /// Creates a keyed template with the credential id set and values at defaults.
    /// Used as the template passed to `ClientAuthCacheStore::fetch`.
    pub fn new_keyed(credential_id: Uuid) -> Self {
        Self {
            credential_id,
            membership_id: Uuid::nil(),
            account_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            roles: vec![],
            permissions: vec![],
            expires_at: None,
            status: CredentialStatus::Active,
        }
    }
}

impl CacheEntity for ClientAuthCache {
    fn _key() -> (&'static str, &'static str) {
        ("oxauth", "client_id")
    }

    fn key(&self) -> CacheKey {
        let (prefix, name) = Self::_key();
        CacheKey::new(prefix, name, self.credential_id)
    }

    fn new_key(credential_id: impl Display) -> CacheKey {
        let (prefix, name) = Self::_key();
        CacheKey::new(prefix, name, credential_id)
    }
}

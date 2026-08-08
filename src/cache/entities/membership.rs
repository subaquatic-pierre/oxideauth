use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cache::{
    error::{CacheError, CacheResult},
    traits::{CacheEntity, CacheKey},
};

/// The cached membership payload persisted under `oxauth:mem:{membership_id}`.
///
/// This is the membership entity in the cache layer, previously defined in
/// `core::models::membership` as `CachedMembership`. It carries the membership
/// identity plus the resolved role/permission data needed to reconstruct a
/// `CoreCtx` without hitting the database on every authenticated request.
#[derive(Serialize, Debug, Clone, Deserialize)]
pub struct MembershipCache {
    pub id: Uuid,
    pub account_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub role_ids: Vec<Uuid>,
    pub permissions: Vec<String>,
}

impl Default for MembershipCache {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            account_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            project_id: None,
            role_ids: vec![],
            // Usually defaults to empty to deny access unless explicitly populated
            permissions: vec![],
        }
    }
}

impl CacheEntity for MembershipCache {
    fn keys(&self) -> HashMap<String, CacheKey> {
        let mut map = HashMap::new();
        map.insert(
            "membership".into(),
            CacheKey::new("oxauth", "mem", self.id),
        );
        map
    }

    fn from_raw(raw: HashMap<String, Option<String>>) -> CacheResult<Self> {
        let json_str = raw
            .get("membership")
            .and_then(|v| v.as_deref())
            .ok_or_else(|| CacheError::NotFound("missing membership cache entry".into()))?;
        serde_json::from_str(json_str).map_err(CacheError::SerdeError)
    }
}

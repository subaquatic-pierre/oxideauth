//! Cache entities for the policy engine.

use std::fmt::Display;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    cache::traits::{CacheEntity, CacheKey},
    core::models::policy::PolicySet,
};

/// The cached per-membership policy set persisted under
/// `oxauth:policy:{membership_id}`.
///
/// Carries the compiled [`PolicySet`] of a membership so that policy evaluation
/// does not hit the database on every request. `mem_id` is the identifier fixed
/// at construction time and used to compute the Redis key; `policies` is the
/// cached value populated after a `fetch` (cache hit) or hydration (cache miss).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCache {
    // Identifier (set at construction)
    pub mem_id: Uuid,

    // Cached value (populated after fetch/hydrate)
    pub policies: PolicySet,
}

impl PolicyCache {
    pub fn new(mem_id: Uuid, policies: PolicySet) -> Self {
        Self { mem_id, policies }
    }
}

impl CacheEntity for PolicyCache {
    fn _key() -> (&'static str, &'static str) {
        ("oxauth", "policy")
    }

    fn key(&self) -> CacheKey {
        let (prefix, name) = Self::_key();
        CacheKey::new(prefix, name, self.mem_id)
    }

    fn new_key(mem_id: impl Display) -> CacheKey {
        let (prefix, name) = Self::_key();
        CacheKey::new(prefix, name, mem_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::policy::{Policy, PolicyEffect};

    /// Builds a small compiled `PolicySet` for tests.
    fn sample_set() -> PolicySet {
        let mut p = Policy::default();
        p.id = Uuid::new_v4();
        p.actions = vec![
            "membership:update".to_string(),
            "profile:update".to_string(),
        ];
        p.resource = "self".to_string();
        PolicySet::from_policies(vec![p])
    }

    #[test]
    fn test_policy_cache_new() {
        let mem_id = Uuid::new_v4();
        let set = sample_set();
        let cache = PolicyCache::new(mem_id, set.clone());

        assert_eq!(cache.mem_id, mem_id);
        assert_eq!(cache.policies, set);
    }

    #[test]
    fn test_policy_cache_key_format() {
        let mem_id = Uuid::new_v4();
        let cache = PolicyCache::new(mem_id, sample_set());

        assert_eq!(cache.key().as_ref(), format!("oxauth:policy:{}", mem_id));
        assert_eq!(
            PolicyCache::new_key(mem_id).as_ref(),
            format!("oxauth:policy:{}", mem_id)
        );
        assert_eq!(PolicyCache::_key(), ("oxauth", "policy"));
    }

    #[test]
    fn test_policy_cache_serde_roundtrip() {
        let mem_id = Uuid::new_v4();
        let cache = PolicyCache::new(mem_id, sample_set());

        let json = serde_json::to_string(&cache).expect("serialize should succeed");
        let back: PolicyCache = serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(back.mem_id, mem_id);
        assert_eq!(back.policies, cache.policies);
        assert_eq!(
            back.policies.get("membership:update", "self", None),
            Some(PolicyEffect::Allow)
        );
    }
}

use std::str::FromStr;
use std::sync::Arc;
use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::store::crud::Get;
use crate::store::ctx::StoreCtx;
use crate::store::entities::membership::MembershipStatus;
use crate::store::join::GetManyToMany;
use crate::store::manager::StoreManager;
use crate::store::traits::dbx::DbExecutor;
use crate::{
    cache::{
        error::{CacheError, CacheResult},
        traits::{CacheEntity, CacheKey},
    },
    core::{
        error::CoreResult,
        models::{
            permission::PermissionSet,
            token::{RefreshClaims, TokenClaims},
        },
    },
    store::stores::workspace::SYSTEM_CONST,
};

/// The cached auth-scope payload persisted under `oxauth:mem_id:{membership_id}`.
///
/// It carries everything needed to reconstruct a [`CoreCtx`] without hitting
/// the database on every authenticated request.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthScopeCache {
    pub workspace_id: Uuid,
    pub workspace_slug: String,
    pub project_id: Option<Uuid>,
    pub roles: Vec<Uuid>,
    pub permissions: Vec<String>,
}

impl AuthScopeCache {
    pub fn system() -> Self {
        Self {
            workspace_id: Uuid::nil(),
            workspace_slug: SYSTEM_CONST.system_ws_slug.to_string(),
            project_id: None,
            roles: vec![],
            permissions: vec!["*:*".to_string()],
        }
    }

    /// Escalates this scope's permissions for the current request via
    /// [`PermissionSet::with_extended`] (validates each new permission and
    /// appends it). This is the data-level escalation primitive: it extends the
    /// auth scope's permission strings so that `CoreCtx::permissions()` reflects
    /// them on subsequent validations.
    pub fn escalate_perms(&mut self, perms: &[&str]) -> CoreResult<()> {
        let extended = PermissionSet::new(&self.permissions).with_extended(perms)?;
        self.permissions = extended.into_vec();
        Ok(())
    }
}

impl Default for AuthScopeCache {
    fn default() -> Self {
        Self {
            workspace_id: Uuid::nil(),
            workspace_slug: "default".to_string(),
            project_id: None,
            roles: vec![],
            permissions: vec![],
        }
    }
}

/// The auth cache entity for a single token identity.
///
/// `mem_id`/`acc_id`/`sid` are identifiers fixed at construction time and are
/// used to compute the Redis keys. The remaining fields are cached values that
/// are populated after a `fetch` (cache hit) or hydration (cache miss).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCache {
    // Identifiers (set at construction)
    pub mem_id: Uuid,
    pub acc_id: Uuid,
    pub sid: Option<Uuid>,

    // Cached values (populated after fetch/hydrate)
    pub mem_version: u64,
    pub acc_version: u64,
    pub mem_active: bool,
    pub acc_enabled: bool,
    pub auth_scope: AuthScopeCache,
}

impl AuthCache {
    pub fn new_keyed(mem_id: Uuid, acc_id: Uuid, sid: Option<Uuid>) -> Self {
        Self {
            mem_id,
            acc_id,
            sid,
            mem_version: 0,
            acc_version: 0,
            mem_active: false,
            acc_enabled: false,
            auth_scope: AuthScopeCache::default(),
        }
    }

    pub fn bootstrap() -> Self {
        Self {
            mem_id: Uuid::nil(),
            acc_id: Uuid::nil(),
            sid: None,
            mem_version: 0,
            acc_version: 0,
            mem_active: true,
            acc_enabled: true,
            auth_scope: AuthScopeCache::system(),
        }
    }

    pub fn from_claims(token_claims: &TokenClaims) -> Self {
        AuthCache {
            mem_id: token_claims.mem,
            acc_id: token_claims.sub,
            sid: token_claims.sid,
            mem_version: token_claims.mem_ver,
            acc_version: token_claims.acc_ver,
            mem_active: true,
            acc_enabled: true,
            auth_scope: AuthScopeCache {
                workspace_id: token_claims.ws,
                workspace_slug: String::new(),
                project_id: None,
                roles: vec![],
                permissions: vec![],
            },
        }
    }

    /// Hydrates a fully-populated `AuthCache` from the database.
    ///
    /// Loads the membership (with its roles), the account, and every role's
    /// permissions, then packages the result for `AuthCacheStore::write`.
    pub async fn build_from_db<D: DbExecutor>(
        sm: Arc<StoreManager<D>>,
        mem_id: Uuid,
        acc_id: Uuid,
        sid: Option<Uuid>,
    ) -> CacheResult<AuthCache> {
        let store_ctx = StoreCtx::bootstrap();

        // Load the membership (with its roles).
        let mem_with_roles = sm
            .membership
            .get_many_to_many(&store_ctx, &mem_id.into())
            .await?;
        let mem_row = mem_with_roles.membership;
        let workspace_row = sm
            .workspace
            .get(&store_ctx, &mem_row.workspace_id.into())
            .await?;

        // Load the account.
        let acc_row = sm.account.get(&store_ctx, &acc_id.into()).await?;

        // Resolve permissions from the membership's roles.
        let mut permissions: Vec<String> = vec![];
        let mut role_ids: Vec<Uuid> = vec![];
        for role in mem_with_roles.roles.iter() {
            role_ids.push(role.id.into());
            let role_with_perms = sm.role.get_many_to_many(&store_ctx, &role.id).await?;
            for perm in role_with_perms.permissions.iter() {
                let name = perm.name.clone();
                if !permissions.contains(&name) {
                    permissions.push(name);
                }
            }
        }

        let auth_scope = AuthScopeCache {
            workspace_id: mem_row.workspace_id,
            workspace_slug: workspace_row.slug,
            project_id: mem_row.project_id,
            roles: role_ids,
            permissions,
        };

        Ok(AuthCache {
            mem_id,
            acc_id,
            sid,
            mem_version: mem_row.version as u64,
            acc_version: acc_row.version as u64,
            mem_active: mem_row.status == MembershipStatus::Active,
            acc_enabled: acc_row.enabled,
            auth_scope,
        })
    }
}

impl CacheEntity for AuthCache {
    fn _key() -> (&'static str, &'static str) {
        ("oxauth", "mem_id")
    }

    fn key(&self) -> CacheKey {
        let (prefix, name) = AuthCache::_key();
        CacheKey::new(prefix, name, self.mem_id)
    }

    fn new_key(mem_id: impl Display) -> CacheKey {
        let (prefix, name) = AuthCache::_key();
        CacheKey::new(prefix, name, mem_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::token::TokenType;

    #[test]
    fn test_auth_scope_system() {
        let scope = AuthScopeCache::system();
        assert_eq!(scope.permissions, vec!["*:*".to_string()]);
        assert_eq!(scope.workspace_slug, "system");
        assert_eq!(scope.workspace_id, Uuid::nil());
        assert!(scope.project_id.is_none());
        assert!(scope.roles.is_empty());
    }

    #[test]
    fn test_auth_scope_default() {
        let scope = AuthScopeCache::default();
        assert_eq!(scope.workspace_slug, "default");
        assert_eq!(scope.workspace_id, Uuid::nil());
        assert!(scope.permissions.is_empty());
        assert!(scope.roles.is_empty());
        assert!(scope.project_id.is_none());
    }

    #[test]
    fn test_auth_cache_new_keyed() {
        let mem = Uuid::new_v4();
        let acc = Uuid::new_v4();
        let sid = Some(Uuid::new_v4());
        let cache = AuthCache::new_keyed(mem, acc, sid);

        assert_eq!(cache.mem_id, mem);
        assert_eq!(cache.acc_id, acc);
        assert_eq!(cache.sid, sid);
        assert_eq!(cache.mem_version, 0);
        assert_eq!(cache.acc_version, 0);
        assert!(!cache.mem_active, "keyed template starts inactive");
        assert!(!cache.acc_enabled, "keyed template starts disabled");
    }

    #[test]
    fn test_auth_cache_bootstrap() {
        let cache = AuthCache::bootstrap();
        assert!(cache.mem_active, "bootstrap membership is active");
        assert!(cache.acc_enabled, "bootstrap account is enabled");
        assert_eq!(cache.mem_id, Uuid::nil());
        assert_eq!(cache.acc_id, Uuid::nil());
        assert_eq!(cache.sid, None);
        assert_eq!(cache.auth_scope.permissions, vec!["*:*".to_string()]);
        assert_eq!(cache.auth_scope.workspace_slug, "system");
    }

    #[test]
    fn test_auth_cache_from_claims_maps_fields() {
        let mem = Uuid::new_v4();
        let acc = Uuid::new_v4();
        let ws = Uuid::new_v4();
        let sid = Uuid::new_v4();

        let claims = TokenClaims {
            sub: acc,
            ws,
            mem,
            iss: "iss".into(),
            aud: "aud".into(),
            exp: 0,
            iat: 0,
            ty: TokenType::Auth,
            mem_ver: 7,
            acc_ver: 8,
            sid: Some(sid),
            jti: None,
        };

        let cache = AuthCache::from_claims(&claims);
        assert_eq!(cache.mem_id, mem);
        assert_eq!(cache.acc_id, acc);
        assert_eq!(cache.sid, Some(sid));
        assert_eq!(cache.mem_version, 7);
        assert_eq!(cache.acc_version, 8);
        assert!(cache.mem_active, "claims-derived membership is active");
        assert!(cache.acc_enabled, "claims-derived account is enabled");
        assert_eq!(cache.auth_scope.workspace_id, ws);
        assert_eq!(cache.auth_scope.workspace_slug, "");
    }

    #[test]
    fn test_auth_cache_from_claims_without_sid() {
        let mem = Uuid::new_v4();
        let claims = TokenClaims {
            sub: Uuid::new_v4(),
            ws: Uuid::new_v4(),
            mem,
            iss: String::new(),
            aud: String::new(),
            exp: 0,
            iat: 0,
            ty: TokenType::Auth,
            mem_ver: 0,
            acc_ver: 0,
            sid: None,
            jti: Some(Uuid::new_v4()),
        };

        let cache = AuthCache::from_claims(&claims);
        assert_eq!(cache.mem_id, mem);
        assert_eq!(cache.sid, None);
    }

    #[test]
    fn test_auth_cache_key_format() {
        let mem = Uuid::new_v4();
        let cache = AuthCache::new_keyed(mem, Uuid::new_v4(), None);

        assert_eq!(cache.key().as_ref(), format!("oxauth:mem_id:{}", mem));
        assert_eq!(AuthCache::new_key(mem).as_ref(), format!("oxauth:mem_id:{}", mem));
        assert_eq!(AuthCache::_key(), ("oxauth", "mem_id"));
    }
}

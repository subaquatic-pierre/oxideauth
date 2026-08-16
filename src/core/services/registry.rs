use std::sync::Arc;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    config::Config,
    core::{
        ctx::ContextFactory,
        services::{
            account::AccountService,
            auth::AuthService,
            client::ClientService,
            credential::CredentialService,
            membership::MembershipService,
            permission::PermissionService,
            project::ProjectService,
            role::RoleService,
            token::{TokenService, TokenServiceConfig},
            validator::AuthValidator,
            workspace::WorkspaceService,
        },
    },
    store::{manager::StoreManager, traits::dbx::DbExecutor},
};

/// Central service registry. Creates every service once at startup and shares
/// them via `Arc`. Replaces the transient `ServiceFactory` pattern — per-request
/// allocation is eliminated, and the full dependency graph is explicit in one
/// constructor.
pub struct ServiceRegistry<D: DbExecutor, C: CacheExecutor> {
    pub sm: Arc<StoreManager<D>>,
    pub cm: Arc<CacheManager<C>>,

    pub workspace: Arc<WorkspaceService<D, C>>,
    pub role: Arc<RoleService<D, C>>,
    pub permission: Arc<PermissionService<D, C>>,
    pub account: Arc<AccountService<D, C>>,
    pub project: Arc<ProjectService<D, C>>,
    pub token: Arc<TokenService<D, C>>,
    pub credential: Arc<CredentialService<D, C>>,
    pub membership: Arc<MembershipService<D, C>>,
    pub client: Arc<ClientService<D, C>>,
    pub auth: Arc<AuthService<D, C>>,
    pub ctx_factory: Arc<ContextFactory>,
    pub auth_validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> ServiceRegistry<D, C> {
    /// Creates every service in topological dependency order.
    pub fn new(config: &Config, sm: Arc<StoreManager<D>>, cm: Arc<CacheManager<C>>) -> Self {
        let token_config = TokenServiceConfig::new(
            config.jwt_secret.clone(),
            config.access_token_max_age,
            config.refresh_token_max_age,
        );

        let ctx_factory = Arc::new(ContextFactory::new());
        let auth_validator = Arc::new(AuthValidator::new());

        // Phase 1: construct in dependency order (leaves first).
        let workspace = Arc::new(WorkspaceService::new(
            sm.clone(),
            cm.clone(),
            auth_validator.clone(),
        ));
        let permission = Arc::new(PermissionService::new(
            sm.clone(),
            workspace.clone(),
            cm.clone(),
            auth_validator.clone(),
        ));
        let role = Arc::new(RoleService::new(
            sm.clone(),
            workspace.clone(),
            permission.clone(),
            cm.clone(),
            auth_validator.clone(),
        ));
        let account = Arc::new(AccountService::new(
            sm.clone(),
            cm.clone(),
            workspace.clone(),
            auth_validator.clone(),
        ));
        let project = Arc::new(ProjectService::new(
            sm.clone(),
            workspace.clone(),
            auth_validator.clone(),
        ));
        let token = Arc::new(TokenService::new(workspace.clone(), token_config));
        let credential = Arc::new(CredentialService::new(
            sm.clone(),
            workspace.clone(),
            account.clone(),
            auth_validator.clone(),
        ));
        let membership = Arc::new(MembershipService::new(
            sm.clone(),
            cm.clone(),
            workspace.clone(),
            account.clone(),
            role.clone(),
            auth_validator.clone(),
        ));
        let client = Arc::new(ClientService::new(
            sm.clone(),
            workspace.clone(),
            cm.clone(),
            auth_validator.clone(),
        ));
        let auth = Arc::new(AuthService::new(
            sm.clone(),
            cm.clone(),
            workspace.clone(),
            account.clone(),
            token.clone(),
            credential.clone(),
            membership.clone(),
            ctx_factory.clone(),
            role.clone(),
            config.clone(),
            auth_validator.clone(),
        ));

        // Phase 2: wire cycle-breaking Weak references into WorkspaceService.
        workspace.wire_permission_service(&permission);
        workspace.wire_role_service(&role);
        workspace.wire_project_service(&project);

        Self {
            sm,
            cm,
            workspace,
            role,
            permission,
            account,
            project,
            token,
            credential,
            membership,
            ctx_factory,
            client,
            auth,
            auth_validator,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{manager::CacheManager, mock::MockChx},
        config::Config,
        core::{
            ctx::CoreCtx,
            error::CoreResult,
            models::token::{TokenClaims, TokenType},
        },
        store::dbx::MockDbx,
        utils::time::now_utc,
    };
    use serial_test::serial;
    use time::Duration as TimeDuration;
    use uuid::Uuid;

    /// Constructs a `ServiceRegistry` backed by in-memory `MockDbx` + `MockChx`
    /// so the constructor wiring can be exercised without a real DB or Redis.
    #[tokio::test]
    #[serial]
    async fn test_service_registry_constructs_all_services() -> CoreResult<()> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(MockDbx::new())));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));

        let registry = ServiceRegistry::new(&config, sm.clone(), cm.clone());

        // The store/cache managers are the same instances that were passed in.
        assert!(Arc::ptr_eq(&registry.sm, &sm));
        assert!(Arc::ptr_eq(&registry.cm, &cm));

        // Every service field must be present and wired.
        let _ = registry.workspace.as_ref();
        let _ = registry.role.as_ref();
        let _ = registry.permission.as_ref();
        let _ = registry.account.as_ref();
        let _ = registry.project.as_ref();
        let _ = registry.token.as_ref();
        let _ = registry.credential.as_ref();
        let _ = registry.membership.as_ref();
        let _ = registry.client.as_ref();
        let _ = registry.auth.as_ref();
        let _ = registry.ctx_factory.as_ref();

        // The token service is fully functional end-to-end (encode -> decode).
        let claims = TokenClaims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now_utc() + TimeDuration::hours(1),
            TokenType::Auth,
            0,
            0,
            None,
            None,
        );
        let encoded = registry.token.encode_token_claims(&claims)?;
        let decoded = registry.token.decode_token_str(&encoded)?;
        assert_eq!(decoded.sub, claims.sub);
        assert_eq!(decoded.ws, claims.ws);
        assert_eq!(decoded.mem, claims.mem);
        assert_eq!(decoded.ty, claims.ty);

        // A bootstrap CoreCtx still resolves with the registry alive.
        let _ctx = CoreCtx::bootstrap()?;

        Ok(())
    }
}

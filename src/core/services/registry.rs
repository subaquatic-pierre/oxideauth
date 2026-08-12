use std::sync::Arc;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    config::Config,
    core::services::{
        account::AccountService,
        auth::AuthService,
        client::ClientService,
        credential::CredentialService,
        membership::MembershipService,
        permission::PermissionService,
        project::ProjectService,
        role::RoleService,
        token::{TokenService, TokenServiceConfig},
        workspace::WorkspaceService,
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
}

impl<D: DbExecutor, C: CacheExecutor> ServiceRegistry<D, C> {
    /// Creates every service in topological dependency order.
    pub fn new(config: &Config, sm: Arc<StoreManager<D>>, cm: Arc<CacheManager<C>>) -> Self {
        let token_config = TokenServiceConfig::new(
            config.jwt_secret.clone(),
            config.access_token_max_age,
            config.refresh_token_max_age,
        );

        // Phase 1: construct in dependency order (leaves first).
        let workspace = Arc::new(WorkspaceService::new(sm.clone()));
        let permission = Arc::new(PermissionService::new(
            sm.clone(),
            workspace.clone(),
            cm.clone(),
        ));
        let role = Arc::new(RoleService::new(
            sm.clone(),
            workspace.clone(),
            permission.clone(),
            cm.clone(),
        ));
        let account = Arc::new(AccountService::new(
            sm.clone(),
            cm.clone(),
            workspace.clone(),
        ));
        let project = Arc::new(ProjectService::new(sm.clone(), workspace.clone()));
        let token = Arc::new(TokenService::new(workspace.clone(), token_config));
        let credential = Arc::new(CredentialService::new(
            sm.clone(),
            workspace.clone(),
            account.clone(),
        ));
        let membership = Arc::new(MembershipService::new(
            sm.clone(),
            cm.clone(),
            workspace.clone(),
            account.clone(),
            role.clone(),
        ));
        let client = Arc::new(ClientService::new(
            sm.clone(),
            workspace.clone(),
            cm.clone(),
        ));
        let auth = Arc::new(AuthService::new(
            sm.clone(),
            cm.clone(),
            workspace.clone(),
            account.clone(),
            token.clone(),
            credential.clone(),
            membership.clone(),
            role.clone(),
            config.clone(),
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
            client,
            auth,
        }
    }
}

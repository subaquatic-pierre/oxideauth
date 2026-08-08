use std::sync::Arc;

use crate::{
    app::AppState,
    cache::{manager::CacheManager, stores::client::ClientRateLimitCache, traits::CacheExecutor},
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

pub struct ServiceFactory<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
}

impl<D, C> ServiceFactory<D, C>
where
    D: DbExecutor,
    C: CacheExecutor,
{
    pub fn new(sm: Arc<StoreManager<D>>, cm: Arc<CacheManager<C>>) -> Self {
        Self { sm, cm }
    }

    pub fn account(&self) -> AccountService<D, C> {
        let svc = AccountService::new(self.sm.clone(), self.cm.clone(), self.workspace());
        svc
    }

    pub fn role(&self) -> RoleService<D, C> {
        let svc = RoleService::new(
            self.sm.clone(),
            self.workspace(),
            self.permission(),
            self.cm.clone(),
        );
        svc
    }

    pub fn permission(&self) -> PermissionService<D, C> {
        let svc = PermissionService::new(self.sm.clone(), self.workspace(), self.cm.clone());
        svc
    }

    pub fn workspace(&self) -> WorkspaceService<D> {
        let svc = WorkspaceService::new(self.sm.clone());
        svc
    }

    pub fn project(&self) -> ProjectService<D> {
        let svc = ProjectService::new(self.sm.clone());
        svc
    }

    pub fn membership(&self) -> MembershipService<D, C> {
        let svc = MembershipService::new(
            self.sm.clone(),
            self.cm.clone(),
            self.workspace(),
            self.account(),
            self.role(),
        );
        svc
    }

    pub fn credential(&self) -> CredentialService<D, C> {
        let svc = CredentialService::new(self.sm.clone(), self.workspace(), self.account());
        svc
    }

    pub fn auth(&self) -> AuthService<D, C> {
        AuthService::new(
            self.sm.clone(),
            self.account(),
            self.token(),
            self.cm.clone(),
            Config::from_env(), // TODO: hold Config in ServiceFactory instead of re-reading from env
        )
    }

    pub fn client(&self) -> ClientService<D, C> {
        // Client validation rate limiting: 5 attempts per 300s window, matching
        // the login attempt policy. TODO: make these configurable.
        let rate_limit = Arc::new(ClientRateLimitCache::new(
            self.cm.executor(),
            5,
            300,
        ));
        let svc = ClientService::new(
            self.sm.clone(),
            self.workspace(),
            self.cm.clone(),
            Some(rate_limit),
            Config::from_env(), // TODO: hold Config in ServiceFactory instead of re-reading from env
        );
        svc
    }

    pub fn token(&self) -> TokenService<D, C> {
        // TODO: get config from storage, first check cache,
        // if not found then check database and update cache
        // the reason for holding config in storage is to allow
        // dynamic config retrieval at runtime, this allows
        // multi tenant configs, also allows for config edit from client
        let config = Config::from_env(); // TODO: hold Config in ServiceFactory instead of re-reading from env
        let token_config = TokenServiceConfig::new(
            config.jwt_secret.clone(),
            config.access_token_max_age,
            config.refresh_token_max_age,
        );
        let svc = TokenService::new(self.cm.clone(), self.workspace(), token_config);
        svc
    }
}

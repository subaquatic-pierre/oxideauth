use std::sync::Arc;

use crate::{
    app::AppState,
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::services::{
        account::AccountService,
        auth::AuthService,
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

    pub fn account(&self) -> AccountService<D> {
        let svc = AccountService::new(self.sm.clone(), self.workspace());
        svc
    }

    pub fn role(&self) -> RoleService<D> {
        let svc = RoleService::new(self.sm.clone(), self.workspace(), self.permission());
        svc
    }

    pub fn permission(&self) -> PermissionService<D> {
        let svc = PermissionService::new(self.sm.clone(), self.workspace());
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

    pub fn credential(&self) -> CredentialService<D> {
        let svc = CredentialService::new(self.sm.clone(), self.workspace(), self.account());
        svc
    }

    pub fn auth(&self) -> AuthService<D> {
        let acc_svc = self.account();
        let svc = AuthService::new(acc_svc);
        svc
    }

    pub fn token(&self) -> TokenService<D, C> {
        // TODO: get config from storage, first check cache,
        // if not found then check database and update cache
        // the reason for holding config in storage is to allow
        // dynamic config retrieval at runtime, this allows
        // multi tenant configs, also allows for config edit from client
        let config = TokenServiceConfig::default();
        let svc = TokenService::new(self.sm.clone(), self.cm.clone(), self.workspace(), config);
        svc
    }
}

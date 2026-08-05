use std::sync::Arc;

use crate::store::{
    dbx::PgDbx,
    init::PgPool,
    stores::{
        account::AccountStore, credential::CredentialStore, membership::MembershipStore,
        permission::PermissionStore, project::ProjectStore, role::RoleStore,
        token::TokenStore, workspace::WorkspaceStore,
    },
    traits::dbx::DbExecutor,
};

pub struct StoreManager<D: DbExecutor> {
    pub dbx: Arc<D>,

    pub account: AccountStore<D>,
    pub credential: CredentialStore<D>,
    pub membership: MembershipStore<D>,
    pub workspace: WorkspaceStore<D>,
    pub permission: PermissionStore<D>,
    pub project: ProjectStore<D>,
    pub role: RoleStore<D>,
    pub token: TokenStore<D>,
}

impl<D: DbExecutor> StoreManager<D> {
    pub fn new(dbx: Arc<D>) -> Self {
        let account = AccountStore::new(dbx.clone());
        let credential = CredentialStore::new(dbx.clone());
        let membership = MembershipStore::new(dbx.clone());
        let workspace = WorkspaceStore::new(dbx.clone());
        let permission = PermissionStore::new(dbx.clone());
        let project = ProjectStore::new(dbx.clone());
        let role = RoleStore::new(dbx.clone());
        let token = TokenStore::new(dbx.clone());

        Self {
            dbx: dbx.clone(),
            account,
            credential,
            membership,
            workspace,
            permission,
            project,
            role,
            token,
        }
    }

    pub fn dbx(&self) -> Arc<D> {
        self.dbx.clone()
    }
}

pub trait StoreManagerTrait<D: DbExecutor> {
    fn account(&self) -> &AccountStore<D>;
}

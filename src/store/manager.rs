use std::sync::Arc;

use crate::store::{
    dbx::PgDbx,
    init::PgPool,
    stores::{
        account::AccountStore, client::ClientStore, credential::CredentialStore,
        membership::MembershipStore, permission::PermissionStore, policy::PolicyStore,
        profile::ProfileStore, project::ProjectStore, role::RoleStore, workspace::WorkspaceStore,
    },
    traits::dbx::DbExecutor,
};

pub struct StoreManager<D: DbExecutor> {
    pub dbx: Arc<D>,

    pub account: AccountStore<D>,
    pub client: ClientStore<D>,
    pub credential: CredentialStore<D>,
    pub membership: MembershipStore<D>,
    pub workspace: WorkspaceStore<D>,
    pub permission: PermissionStore<D>,
    pub profile: ProfileStore<D>,
    pub project: ProjectStore<D>,
    pub role: RoleStore<D>,
    pub policy: PolicyStore<D>,
}

impl<D: DbExecutor> StoreManager<D> {
    pub fn new(dbx: Arc<D>) -> Self {
        let account = AccountStore::new(dbx.clone());
        let client = ClientStore::new(dbx.clone());
        let credential = CredentialStore::new(dbx.clone());
        let membership = MembershipStore::new(dbx.clone());
        let workspace = WorkspaceStore::new(dbx.clone());
        let permission = PermissionStore::new(dbx.clone());
        let profile = ProfileStore::new(dbx.clone());
        let project = ProjectStore::new(dbx.clone());
        let role = RoleStore::new(dbx.clone());
        let policy = PolicyStore::new(dbx.clone());

        Self {
            dbx: dbx.clone(),
            account,
            client,
            credential,
            membership,
            workspace,
            permission,
            profile,
            project,
            role,
            policy,
        }
    }

    pub fn dbx(&self) -> Arc<D> {
        self.dbx.clone()
    }
}

pub trait StoreManagerTrait<D: DbExecutor> {
    fn account(&self) -> &AccountStore<D>;
}


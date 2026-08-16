use std::sync::Arc;

use crate::store::{
    dbx::PgDbx,
    init::PgPool,
    stores::{
        account::AccountStore, client::ClientStore, credential::CredentialStore,
        membership::MembershipStore, permission::PermissionStore, policy::PolicyStore,
        membership::MembershipStore, permission::PermissionStore, profile::ProfileStore,
        project::ProjectStore, role::RoleStore, workspace::WorkspaceStore,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{dbx::MockDbx, traits::meta::Store};

    #[test]
    fn test_store_manager_new_with_mock_dbx() {
        // -- Setup
        let dbx = Arc::new(MockDbx::new());

        // -- Execute
        let manager = StoreManager::new(dbx.clone());

        // -- Assert
        // dbx() returns an Arc clone of the same underlying executor
        assert!(Arc::ptr_eq(&manager.dbx(), &dbx));

        // All sub-stores must be constructed and accessible
        let _ = &manager.account;
        let _ = &manager.client;
        let _ = &manager.credential;
        let _ = &manager.membership;
        let _ = &manager.workspace;
        let _ = &manager.permission;
        let _ = &manager.profile;
        let _ = &manager.project;
        let _ = &manager.role;
        let _ = &manager.policy;

        // The Store trait is implemented for each sub-store (dbx() accessor works)
        let _ = manager.account.dbx();
        let _ = manager.client.dbx();
        let _ = manager.credential.dbx();
        let _ = manager.membership.dbx();
        let _ = manager.workspace.dbx();
        let _ = manager.permission.dbx();
        let _ = manager.profile.dbx();
        let _ = manager.project.dbx();
        let _ = manager.role.dbx();
        let _ = manager.policy.dbx();
    }
}

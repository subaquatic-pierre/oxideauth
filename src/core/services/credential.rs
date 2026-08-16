use uuid::Uuid;

use crate::{
    cache::traits::CacheExecutor,
    core::models::{
        permission::PermissionRule,
        workspace::{Workspace, WorkspaceDescribeParams},
    },
    store::{
        entities::credential::{CredentialForCreate, CredentialForUpdate, CredentialRow},
        error::StoreError,
        manager::StoreManager,
        stores::credential::CredentialStore,
        traits::{crud::*, dbx::DbExecutor},
    },
};
use crate::{
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{Account, AccountDescribeParams},
            credential::{
                Credential, CredentialCreateParams, CredentialDeleteParams,
                CredentialDescribeParams, CredentialListParams, CredentialUpdateParams,
            },
            list::ListResponse,
        },
        services::{
            account::AccountService, validator::AuthValidator, permission::CANONICAL_PERMISSIONS,
            workspace::WorkspaceService,
        },
        traits::{
            list::RequestListParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::contains::FilterByContains,
};
use std::{collections::HashMap, sync::Arc};

pub struct CredentialService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    acc_svc: Arc<AccountService<D, C>>,
}

impl<D: DbExecutor, C: CacheExecutor> CredentialService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        acc_svc: Arc<AccountService<D, C>>,
    ) -> Self {
        Self {
            sm,
            ws_svc,
            acc_svc,
        }
    }

    async fn get_account(
        &self,
        ctx: &mut CoreCtx,
        id: Uuid,
        _workspace_id: Uuid,
    ) -> CoreResult<Account> {
        ctx.extend_perms(&["account:describe"])?;

        let account = self
            .acc_svc
            .describe(
                ctx,
                AccountDescribeParams {
                    email: None,
                    id: Some(id),
                },
            )
            .await?;
        Ok(account)
    }
}

// --- Base Model Service ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for CredentialService<D, C> {
    type CoreModel = Credential;
    type ServiceStore = CredentialStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.credential
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }
}

// --- Create ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for CredentialService<D, C> {
    type CreateParams = CredentialCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.create;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        let for_create: CredentialForCreate = params.clone().into();

        let row = match store.create(&store_ctx, for_create).await {
            Ok(row) => row,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::AlreadyExists(
                    "a credential of this kind already exists for this account in this workspace"
                        .to_string(),
                ));
            }
            Err(e) => return Err(e.into()),
        };

        Ok(row.into())
    }
}

// --- Describe (with Hydration) ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for CredentialService<D, C> {
    type DescribeParams = CredentialDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let row = store.get(&store_ctx, &params.id.into()).await?;

        Ok(row.into())
    }
}

// --- List ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for CredentialService<D, C> {
    type ListParams = CredentialListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>> {
        let store = self.store();

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
            .await?;
        // validate params
        let list_options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;

        // Combined query: tags (@> containment) + field filter
        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_with_tags_and_filter(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(list_options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let mut credentials: Vec<Credential> = data.into_iter().map(|v| v.into()).collect();

        Ok(ListResponse::new(credentials, total, list_options))
    }
}

// --- Update ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for CredentialService<D, C> {
    type UpdateParams = CredentialUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.update;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // TODO: invalidate auth_cache for any memberships using this credential

        let for_update: CredentialForUpdate = params.clone().into();

        store
            .update(&store_ctx, &params.id.into(), for_update)
            .await?;

        self.describe(
            ctx,
            CredentialDescribeParams {
                id: params.id,
                account_id: params.account_id,
                workspace_id: params.workspace_id,
                provider_id: None,
                email: None,
            },
        )
        .await
    }
}

// --- Delete ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for CredentialService<D, C> {
    type DeleteParams = CredentialDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.delete;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        let to_delete = self
            .describe(
                ctx,
                CredentialDescribeParams {
                    id: params.id,
                    account_id: params.account_id,
                    workspace_id: params.workspace_id,
                    provider_id: None,
                    email: None,
                },
            )
            .await?;

        // TODO: invalidate auth_cache for any memberships using this credential

        let res = store.delete(&store_ctx, &params.id.into()).await?;

        Ok(to_delete)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{manager::CacheManager, mock::MockChx},
        config::Config,
        core::services::registry::ServiceRegistry,
        store::{
            dbx::MockDbx,
            entities::{
                account::AccountRow,
                audit::AuditFields,
                credential::{
                    CredentialConfig, CredentialKind, CredentialMeta, CredentialProvider,
                    CredentialStatus,
                },
                id::DbId,
                workspace::WorkspaceRow,
            },
        },
    };
    use serial_test::serial;
    use uuid::Uuid;

    /// Builds a `CredentialRow` for the in-memory mock with consistent ids.
    fn credential_row(id: Uuid, account_id: Uuid, ws_id: Uuid) -> CredentialRow {
        CredentialRow {
            id: id.into(),
            account_id: account_id.into(),
            workspace_id: ws_id.into(),
            kind: CredentialKind::Password,
            provider: CredentialProvider::Local,
            status: CredentialStatus::Active,
            provider_id: None,
            email: None,
            secret: None,
            last_used_at: None,
            config: CredentialConfig::default(),
            tags: vec![],
            meta: CredentialMeta::default(),
            audit: AuditFields::default(),
        }
    }

    /// Builds a `CredentialService` backed by an in-memory `MockDbx` + `MockChx`.
    fn mock_svc(dbx: MockDbx) -> Arc<CredentialService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.credential.clone()
    }

    fn ws_row(ws_id: Uuid) -> WorkspaceRow {
        WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        }
    }

    fn account_row(account_id: Uuid) -> AccountRow {
        AccountRow {
            id: account_id.into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_credential_create() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let cred_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // create -> store.create
            .with_one::<CredentialRow>(credential_row(cred_id, account_id, ws_id))
            // create -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> store.get
            .with_optional::<CredentialRow>(Some(credential_row(cred_id, account_id, ws_id)))
            // describe -> get_account -> account describe -> store.get
            .with_optional::<AccountRow>(Some(account_row(account_id)))
            // describe -> ws_svc.describe -> get_workspace_by_slug_or_id
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> ws_svc.describe -> store.get
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["credential:create", "credential:describe"])?;

        let params = CredentialCreateParams {
            account_id,
            workspace_id: ws_id,
            kind: CredentialKind::Password,
            provider: CredentialProvider::Local,
            status: CredentialStatus::Active,
            provider_id: None,
            email: Some("user@example.com".to_string()),
            secret: Some("hashed-secret".to_string()),
            config: CredentialConfig::default(),
            last_used_at: None,
            tags: vec![],
            meta: CredentialMeta::default(),
        };

        let credential = svc.create(&mut ctx, params).await?;

        assert_eq!(credential.id, cred_id);
        assert_eq!(credential.account_id, account_id);
        assert_eq!(credential.workspace_id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_credential_describe() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let cred_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // store.get
            .with_optional::<CredentialRow>(Some(credential_row(cred_id, account_id, ws_id)))
            // get_account -> account describe -> store.get
            .with_optional::<AccountRow>(Some(account_row(account_id)))
            // ws_svc.describe -> get_workspace_by_slug_or_id
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // ws_svc.describe -> store.get
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["credential:describe"])?;

        let credential = svc
            .describe(
                &mut ctx,
                CredentialDescribeParams {
                    id: cred_id,
                    account_id,
                    workspace_id: ws_id,
                    provider_id: None,
                    email: None,
                },
            )
            .await?;

        assert_eq!(credential.id, cred_id);
        assert_eq!(credential.account_id, account_id);
        assert_eq!(credential.workspace_id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_credential_list() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let cred_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // list_with_tags_and_filter
            .with_all::<CredentialRow>(vec![credential_row(cred_id, account_id, ws_id)])
            // count_with_tags_and_filter
            .with_one::<(i64,)>((1,))
            // hydrate -> ws_svc.describe -> get_workspace_by_slug_or_id
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // hydrate -> ws_svc.describe -> store.get
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // hydrate -> get_account -> account describe -> store.get
            .with_optional::<AccountRow>(Some(account_row(account_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["credential:list"])?;

        let res = svc
            .list(
                &mut ctx,
                CredentialListParams {
                    workspace_id: ws_id,
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id, cred_id);
        assert_eq!(res.data[0].account_id, account_id);
        assert_eq!(res.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_credential_update() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let cred_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // update -> store.update
            .with_optional::<CredentialRow>(Some(credential_row(cred_id, account_id, ws_id)))
            // describe -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> store.get
            .with_optional::<CredentialRow>(Some(credential_row(cred_id, account_id, ws_id)))
            // describe -> get_account -> store.get
            .with_optional::<AccountRow>(Some(account_row(account_id)))
            // describe -> ws_svc.describe -> get_workspace_by_slug_or_id
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // describe -> ws_svc.describe -> store.get
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["credential:update", "credential:describe"])?;

        let params = CredentialUpdateParams {
            id: cred_id,
            provider_id: None,
            email: None,
            account_id,
            workspace_id: ws_id,
            kind: None,
            provider: None,
            status: None,
            new_provider_id: None,
            new_email: None,
            secret: None,
            last_used_at: None,
            config: None,
            tags: None,
            meta: None,
        };

        let credential = svc.update(&mut ctx, params).await?;

        assert_eq!(credential.id, cred_id);
        assert_eq!(credential.account_id, account_id);
        assert_eq!(credential.workspace_id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_credential_delete() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let cred_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // delete -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // delete -> describe -> scope_and_validate_ctx -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // delete -> describe -> store.get
            .with_optional::<CredentialRow>(Some(credential_row(cred_id, account_id, ws_id)))
            // delete -> describe -> get_account -> store.get
            .with_optional::<AccountRow>(Some(account_row(account_id)))
            // delete -> describe -> ws_svc.describe -> get_workspace_by_slug_or_id
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // delete -> describe -> ws_svc.describe -> store.get
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // delete -> store.delete
            .with_optional::<CredentialRow>(Some(credential_row(cred_id, account_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.extend_perms(&["credential:delete", "credential:describe"])?;

        let credential = svc
            .delete(
                &mut ctx,
                CredentialDeleteParams {
                    id: cred_id,
                    account_id,
                    workspace_id: ws_id,
                    provider_id: None,
                    email: None,
                },
            )
            .await?;

        assert_eq!(credential.id, cred_id);
        assert_eq!(credential.account_id, account_id);

        Ok(())
    }
}

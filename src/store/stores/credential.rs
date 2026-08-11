use std::sync::Arc;

use crate::store::{
    dbx::PgDbx,
    entities::credential::{
        CredentialFilter, CredentialForCreate, CredentialForUpdate, CredentialIden, CredentialRow,
    },
    queries::meta::{ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta},
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Credential store, holding the database connection wrapper.
pub struct CredentialStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> CredentialStore<D> {
    /// Creates a new `CredentialStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        // Use generic
        Self { dbx }
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, CredentialStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for CredentialStore<D> {
    type Iden = CredentialIden;
    type Row = CredentialRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for CredentialStore<D> {
    type FilterStoreParams = CredentialFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: CredentialIden::Table,
            pk: CredentialIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for CredentialStore<D> {
    type CreateStoreParams = CredentialForCreate;
    type UpdateStoreParams = CredentialForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: CredentialIden::Table,
            pk: CredentialIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for CredentialStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: CredentialIden::Table,
            col: CredentialIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: CredentialIden::Table,
            col: CredentialIden::Meta,
            has_audit: true,
        }
    }
}

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cache::redis::RedisChx,
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{
                account::AccountForCreate,
                credential::{CredentialKind, CredentialProvider, CredentialStatus},
                workspace::WorkspaceForCreate,
            },
            error::StoreError,
            traits::{contains::FilterByContains, crud::*},
        },
    };
    use anyhow::Result;
    use serde_json::json;
    use serial_test::serial;
    use uuid::Uuid;

    /// Helper function to seed the necessary Account and Workspace for a Credential.
    async fn seed_prerequisites(
        ctx: &StoreCtx,
        app: &crate::app::AppState<PgDbx, RedisChx>,
    ) -> Result<(Uuid, Uuid)> {
        let account = app
            .sm
            .account
            .create(ctx, AccountForCreate::default())
            .await?;
        let workspace = app
            .sm
            .workspace
            .create(ctx, WorkspaceForCreate::default())
            .await?;
        Ok((account.id.into(), workspace.id.into()))
    }

    #[tokio::test]
    #[serial]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // Prerequisite: Create an account and workspace to link the credential to.
        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;

        let mut data = CredentialForCreate::default();
        data.account_id = account_id;
        data.workspace_id = workspace_id;

        data.provider = CredentialProvider::Google;

        // -- Execute
        let created_cred = store.create(&ctx, data).await?;
        let fetched_cred = store.get(&ctx, &created_cred.id).await?;

        // -- Assert
        assert_eq!(created_cred.account_id, account_id.into());
        assert_eq!(created_cred.workspace_id, workspace_id.into());
        assert_eq!(created_cred.provider, CredentialProvider::Google);
        assert_eq!(fetched_cred.id, created_cred.id);
        assert_eq!(fetched_cred.provider, created_cred.provider);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;
        let mut data = CredentialForCreate::default();
        data.account_id = account_id;
        data.workspace_id = workspace_id;
        let created_cred = store.create(&ctx, data).await?;

        let update_data = CredentialForUpdate {
            status: Some(CredentialStatus::Revoked),
            ..Default::default()
        };

        // -- Execute
        let updated_cred = store.update(&ctx, &created_cred.id, update_data).await?;
        let fetched_cred = store.get(&ctx, &created_cred.id).await?;

        // -- Assert
        assert_eq!(updated_cred.status, CredentialStatus::Revoked);
        assert_eq!(fetched_cred.status, CredentialStatus::Revoked);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;
        let mut data = CredentialForCreate::default();
        data.account_id = account_id;
        data.workspace_id = workspace_id;
        let created_cred = store.create(&ctx, data).await?;

        // -- Execute
        let deleted_cred = store.delete(&ctx, &created_cred.id).await?;
        let get_result = store.get(&ctx, &created_cred.id).await;

        // -- Assert
        assert_eq!(deleted_cred.id, created_cred.id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the credential after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;

        let creds_to_create = vec![
            CredentialForCreate {
                account_id,
                workspace_id,
                provider: CredentialProvider::Local,
                kind: CredentialKind::ApiKey,
                ..Default::default()
            },
            CredentialForCreate {
                account_id,
                workspace_id,
                secret: Some("cool".to_string()),
                provider: CredentialProvider::Google,
                ..Default::default()
            },
        ];
        store.create_many(&ctx, creds_to_create).await?;

        // -- Execute
        let filter: CredentialFilter =
            json!({"provider":CredentialProvider::Google.to_string(),"secret":"cool"})
                .try_into()?;
        let credentials = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].provider, CredentialProvider::Google);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let (account_id, workspace_id) = seed_prerequisites(&ctx, &app).await?;

        let creds_to_create = vec![
            CredentialForCreate {
                account_id,
                workspace_id,
                tags: vec!["primary".into(), "oauth".into()],
                kind: CredentialKind::ApiKey,
                ..Default::default()
            },
            CredentialForCreate {
                account_id,
                workspace_id,
                tags: vec!["secondary".into(), "mfa".into()],
                ..Default::default()
            },
        ];
        store.create_many(&ctx, creds_to_create).await?;

        // -- Execute & Assert
        let primary_creds = store
            .filter_by_tags_contain(&ctx, vec!["primary".into()], None)
            .await?;
        assert_eq!(
            primary_creds.len(),
            1,
            "Should find 1 credential with 'primary' tag"
        );

        let mfa_creds = store
            .filter_by_tags_contain(&ctx, vec!["mfa".into()], None)
            .await?;
        assert_eq!(
            mfa_creds.len(),
            1,
            "Should find 1 credential with 'mfa' tag"
        );
        assert!(mfa_creds[0].tags.contains(&"secondary".to_string()));

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests

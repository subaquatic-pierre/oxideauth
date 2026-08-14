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
    use crate::store::{
        ctx::StoreCtx,
        dbx::MockDbx,
        entities::{
            audit::AuditFields,
            credential::{
                CredentialConfig, CredentialKind, CredentialMeta, CredentialProvider,
                CredentialStatus,
            },
            id::DbId,
        },
        error::StoreError,
        traits::{contains::FilterByContains, crud::*},
    };
    use anyhow::Result;
    use serde_json::json;
    use uuid::Uuid;

    /// Helper to build a `CredentialRow` with default-ish values for the mock.
    fn credential_row() -> CredentialRow {
        CredentialRow {
            id: DbId::from(Uuid::new_v4()),
            account_id: DbId::default(),
            workspace_id: DbId::default(),
            kind: CredentialKind::ApiKey,
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

    #[tokio::test]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let account_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let credential_id = DbId::from(Uuid::new_v4());

        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<CredentialRow>(CredentialRow {
                    id: credential_id,
                    account_id: account_id.into(),
                    workspace_id: workspace_id.into(),
                    provider: CredentialProvider::Google,
                    ..credential_row()
                })
                .with_optional::<CredentialRow>(Some(CredentialRow {
                    id: credential_id,
                    provider: CredentialProvider::Google,
                    ..credential_row()
                })),
        );
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

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
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<CredentialRow>(credential_row())
                .with_optional::<CredentialRow>(Some(CredentialRow {
                    status: CredentialStatus::Revoked,
                    ..credential_row()
                }))
                .with_optional::<CredentialRow>(Some(CredentialRow {
                    status: CredentialStatus::Revoked,
                    ..credential_row()
                })),
        );
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_cred = store.create(&ctx, CredentialForCreate::default()).await?;
        let updated_cred = store
            .update(
                &ctx,
                &created_cred.id,
                CredentialForUpdate {
                    status: Some(CredentialStatus::Revoked),
                    ..Default::default()
                },
            )
            .await?;
        let fetched_cred = store.get(&ctx, &created_cred.id).await?;

        // -- Assert
        assert_eq!(updated_cred.status, CredentialStatus::Revoked);
        assert_eq!(fetched_cred.status, CredentialStatus::Revoked);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let credential_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_optional::<CredentialRow>(Some(CredentialRow {
                    id: credential_id,
                    ..credential_row()
                }))
                .with_optional::<CredentialRow>(None),
        );
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let deleted_cred = store.delete(&ctx, &credential_id).await?;
        let get_result = store.get(&ctx, &credential_id).await;

        // -- Assert
        assert_eq!(deleted_cred.id, credential_id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the credential after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<CredentialRow>(vec![credential_row(), credential_row()])
                .with_all::<CredentialRow>(vec![CredentialRow {
                    provider: CredentialProvider::Google,
                    ..credential_row()
                }]),
        );
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        store
            .create_many(
                &ctx,
                vec![CredentialForCreate::default(), CredentialForCreate::default()],
            )
            .await?;

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
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<CredentialRow>(vec![CredentialRow {
                    tags: vec!["primary".into(), "oauth".into()],
                    ..credential_row()
                }])
                .with_all::<CredentialRow>(vec![CredentialRow {
                    tags: vec!["secondary".into(), "mfa".into()],
                    ..credential_row()
                }]),
        );
        let store = CredentialStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

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

use std::sync::Arc;

use crate::store::{
    entities::client::{ClientFilter, ClientForCreate, ClientForUpdate, ClientIden, ClientRow},
    queries::meta::{ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta},
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Client store, holding the database connection wrapper.
pub struct ClientStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> ClientStore<D> {
    /// Creates a new `ClientStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, ClientStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for ClientStore<D> {
    type Iden = ClientIden;
    type Row = ClientRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for ClientStore<D> {
    type FilterStoreParams = ClientFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: ClientIden::Table,
            pk: ClientIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for ClientStore<D> {
    type CreateStoreParams = ClientForCreate;
    type UpdateStoreParams = ClientForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: ClientIden::Table,
            pk: ClientIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for ClientStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ClientIden::Table,
            col: ClientIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ClientIden::Table,
            col: ClientIden::Meta,
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
            client::ClientMeta,
            id::DbId,
        },
        error::StoreError,
        traits::{contains::FilterByContains, crud::*},
    };
    use anyhow::Result;
    use serde_json::json;
    use uuid::Uuid;

    fn client_row(id: DbId, name: &str) -> ClientRow {
        ClientRow {
            id,
            workspace_id: Uuid::new_v4(),
            name: name.to_string(),
            secret_hash: "test-secret-hash".to_string(),
            endpoint: Some("https://client.example.com".to_string()),
            description: None,
            tags: vec![],
            meta: ClientMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields::default(),
        }
    }

    #[tokio::test]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<ClientRow>(client_row(id, "create-get-client"))
                .with_optional::<ClientRow>(Some(client_row(id, "create-get-client"))),
        );
        let store = ClientStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_client = store.create(&ctx, ClientForCreate::default()).await?;
        let fetched_client = store.get(&ctx, &created_client.id).await?;

        // -- Assert
        assert_eq!(created_client.name, "create-get-client");
        assert_eq!(fetched_client.id, created_client.id);
        assert_eq!(fetched_client.name, "create-get-client");
        assert_eq!(fetched_client.secret_hash, "test-secret-hash");

        Ok(())
    }

    #[tokio::test]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<ClientRow>(client_row(id, "update-client"))
                .with_optional::<ClientRow>(Some(client_row(id, "client-after-update")))
                .with_optional::<ClientRow>(Some(client_row(id, "client-after-update"))),
        );
        let store = ClientStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_client = store.create(&ctx, ClientForCreate::default()).await?;
        let updated_client = store
            .update(
                &ctx,
                &created_client.id,
                ClientForUpdate {
                    name: Some("client-after-update".to_string()),
                    ..Default::default()
                },
            )
            .await?;
        let fetched_client = store.get(&ctx, &created_client.id).await?;

        // -- Assert
        assert_eq!(updated_client.name, "client-after-update");
        assert_eq!(fetched_client.name, "client-after-update");

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let client_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_optional::<ClientRow>(Some(client_row(client_id, "delete-client")))
                .with_optional::<ClientRow>(None),
        );
        let store = ClientStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let deleted_client = store.delete(&ctx, &client_id).await?;
        let get_result = store.get(&ctx, &client_id).await;

        // -- Assert
        assert_eq!(deleted_client.id, client_id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the client after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let id_a = DbId::from(Uuid::new_v4());
        let id_b = DbId::from(Uuid::new_v4());
        let id_c = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<ClientRow>(vec![
                    client_row(id_a, "list-a"),
                    client_row(id_b, "list-b"),
                ])
                .with_all::<ClientRow>(vec![client_row(id_c, "list-filtered")]),
        );
        let store = ClientStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        store
            .create_many(
                &ctx,
                vec![ClientForCreate::default(), ClientForCreate::default()],
            )
            .await?;

        let filter: ClientFilter = json!({"name": {"$contains": "list-filtered"}}).try_into()?;
        let clients = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].name, "list-filtered");

        Ok(())
    }

    #[tokio::test]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let mut tags_client_a = client_row(DbId::from(Uuid::new_v4()), "tags-a");
        tags_client_a.tags = vec!["test-filter-system".into(), "test-filter-critical".into()];
        let mut tags_client_b = client_row(DbId::from(Uuid::new_v4()), "tags-b");
        tags_client_b.tags = vec!["test-filter-user".into(), "test-filter-general".into()];

        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<ClientRow>(vec![tags_client_a])
                .with_all::<ClientRow>(vec![tags_client_b]),
        );
        let store = ClientStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute & Assert
        let system_clients = store
            .filter_by_tags_contain(&ctx, vec!["test-filter-system".into()], None)
            .await?;
        assert_eq!(
            system_clients.len(),
            1,
            "Should find 1 client with 'test-filter-system' tag"
        );
        assert_eq!(system_clients[0].name, "tags-a");

        let general_clients = store
            .filter_by_tags_contain(&ctx, vec!["test-filter-general".into()], None)
            .await?;
        assert_eq!(
            general_clients.len(),
            1,
            "Should find 1 client with 'test-filter-general' tag"
        );
        assert_eq!(general_clients[0].name, "tags-b");

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests

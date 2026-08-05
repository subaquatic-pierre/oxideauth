use std::sync::Arc;

use crate::store::{
    dbx::PgDbx,
    entities::token::{TokenFilter, TokenForCreate, TokenForUpdate, TokenIden, TokenRow},
    queries::meta::{MutateQueryMeta, ReadQueryMeta},
    traits::{
        dbx::DbExecutor,
        meta::{MutateStore, ReadStore, Store},
    },
};
use modql::field::HasSeaFields;

/// The struct for our Token store, holding the database connection wrapper.
pub struct TokenStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> TokenStore<D> {
    /// Creates a new `TokenStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, TokenStore implicitly gains
// its capabilities from the blanket implementations.

impl<D: DbExecutor> Store for TokenStore<D> {
    type Iden = TokenIden;
    type Row = TokenRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for TokenStore<D> {
    type FilterStoreParams = TokenFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: TokenIden::Table,
            pk: TokenIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for TokenStore<D> {
    type CreateStoreParams = TokenForCreate;
    type UpdateStoreParams = TokenForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: TokenIden::Table,
            pk: TokenIden::Id,
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
    use std::str::FromStr;

    use super::*;
    use crate::{
        core::models::workspace,
        dev::{
            fixtures::{global_ws_id, root_user_id},
            init::init_test,
        },
        store::{
            ctx::StoreCtx,
            entities::{hash::Sha256Hash, token::TokenForCreate},
            error::StoreError,
            traits::crud::*,
        },
        utils::time::now_utc,
    };
    use anyhow::Result;
    use serde_json::json;
    use serial_test::serial;
    use time::Duration;
    use uuid::Uuid;

    #[tokio::test]
    #[serial]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = TokenStore::new(dbx);
        let ctx = StoreCtx::new_root();

        let hash = Sha256Hash::gen_rand();
        let hash_2: Sha256Hash = Sha256Hash::new(hash.bytes().clone());

        let data = TokenForCreate {
            hash: hash_2,
            expires_at: now_utc() + Duration::days(1),
            workspace_id: global_ws_id(),
            account_id: root_user_id(),
            ..Default::default()
        };

        // -- Execute
        let created_entry = store.create(&ctx, data).await?;
        let fetched_entry = store.get(&ctx, &created_entry.id).await?;

        // -- Assert
        assert_eq!(created_entry.hash, hash);
        assert_eq!(fetched_entry.id, created_entry.id);
        assert_eq!(fetched_entry.hash, hash);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = TokenStore::new(dbx);
        let ctx = StoreCtx::new_root();

        let hash = Sha256Hash::gen_rand();

        let data = TokenForCreate {
            hash: hash,
            expires_at: now_utc() + Duration::days(1),
            workspace_id: global_ws_id(),
            account_id: root_user_id(),
            ..Default::default()
        };
        let created_entry = store.create(&ctx, data).await?;

        // -- Execute
        let deleted_entry = store.delete(&ctx, &created_entry.id).await?;
        let get_result = store.get(&ctx, &created_entry.id).await;

        // -- Assert
        assert_eq!(deleted_entry.id, created_entry.id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the blacklist entry after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = TokenStore::new(dbx);
        let ctx = StoreCtx::new_root();

        let hash = Sha256Hash::gen_rand();
        let hash_2: Sha256Hash = Sha256Hash::new(hash.bytes().clone());
        let entries_to_create = vec![
            TokenForCreate {
                hash: hash_2,
                expires_at: now_utc() + Duration::days(1),
                reason: Some("REASON".to_string()),
                account_id: root_user_id(),
                workspace_id: global_ws_id(),
                ..Default::default()
            },
            TokenForCreate {
                hash: Sha256Hash::gen_rand(),
                expires_at: now_utc() + Duration::days(1),
                workspace_id: global_ws_id(),
                account_id: root_user_id(),
                ..Default::default()
            },
        ];
        store.create_many(&ctx, entries_to_create).await?;

        // -- Execute
        let filter: TokenFilter = json!({ "reason": "REASON" }).try_into()?;
        let entries = store.list(&ctx, Some(filter), None).await?;

        println!("{entries:#?}");
        // -- Assert
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hash, hash);

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests

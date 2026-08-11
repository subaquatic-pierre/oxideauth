use axum::async_trait;
use sqlx::{
    Execute, FromRow, IntoArguments, Postgres, Transaction,
    query::{Query, QueryAs},
};
use std::{
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex, oneshot},
    task::JoinHandle,
    time::{MissedTickBehavior, interval, sleep},
};
use tracing::{debug, error};

use crate::store::{
    error::{StoreError, StoreResult},
    init::PgPool,
    traits::dbx::DbExecutor,
};

/// PgDbx is a thin wrapper over a sqlx Pool that can (optionally) route all queries
/// through a shared transaction. It also supports *nested* transactions via a
/// simple ref-count on a single physical transaction.
pub struct PgDbx {
    /// Underlying sqlx connection pool.
    db_pool: PgPool,
}

impl PgDbx {
    /// Create a new PgDbx from a pool.
    pub fn new(db: PgPool) -> Self {
        Self { db_pool: db }
    }

    /// Borrow the underlying pool (used when no transaction is active).
    pub fn pool(&self) -> &PgPool {
        &self.db_pool
    }

    /// Borrow the underlying pool (used when no transaction is active).
    pub async fn begin(&self) -> StoreResult<Transaction<'static, Postgres>> {
        Ok(self.db_pool.begin().await?)
    }

    // --- Query Execution methods

    /// Execute a `query_as` and fetch exactly one row.
    /// If a transaction is active, runs against it; otherwise uses the pool.
    pub async fn fetch_one<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<O>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        let data = query.fetch_one(self.pool()).await?;

        Ok(data)
    }

    /// Execute a `query_as` and fetch an optional row.
    /// If a transaction is active, runs against it; otherwise uses the pool.
    pub async fn fetch_optional<'q, O, A>(
        &self,
        query: QueryAs<'q, Postgres, O, A>,
    ) -> StoreResult<Option<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        let data = query.fetch_optional(self.pool()).await?;

        Ok(data)
    }

    /// Execute a `query_as` and fetch all rows.
    /// If a transaction is active, runs against it; otherwise uses the pool.
    pub async fn fetch_all<'q, O, A>(
        &self,
        query: QueryAs<'q, Postgres, O, A>,
    ) -> StoreResult<Vec<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        // No need to debug here, sqlx::query logs debug info
        // debug!("--- QUERY --- : SQL: {}", query.sql());
        let data = query.fetch_all(self.pool()).await?;

        Ok(data)
    }

    /// Execute a `query` (no mapping) and return rows affected.
    /// If a transaction is active, runs against it; otherwise uses the pool.
    pub async fn execute<'q, A>(&self, query: Query<'q, Postgres, A>) -> StoreResult<u64>
    where
        A: IntoArguments<'q, Postgres> + 'q,
    {
        let rows_affected = query.execute(self.pool()).await?.rows_affected();

        Ok(rows_affected)
    }
}

#[async_trait]
impl DbExecutor for PgDbx {
    async fn fetch_one<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<O>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.fetch_one(query).await
    }

    async fn fetch_optional<'q, O, A>(
        &self,
        query: QueryAs<'q, Postgres, O, A>,
    ) -> StoreResult<Option<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.fetch_optional(query).await
    }

    async fn fetch_all<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<Vec<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.fetch_all(query).await
    }

    async fn execute<'q, A>(&self, query: Query<'q, Postgres, A>) -> StoreResult<u64>
    where
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.execute(query).await
    }
}

pub struct MockDbx {}

#[async_trait]
impl DbExecutor for MockDbx {
    async fn fetch_one<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<O>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        Err(StoreError::MockReturn)
    }

    async fn fetch_optional<'q, O, A>(
        &self,
        query: QueryAs<'q, Postgres, O, A>,
    ) -> StoreResult<Option<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        Ok(None)
    }

    async fn fetch_all<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<Vec<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        Ok(vec![])
    }

    async fn execute<'q, A>(&self, query: Query<'q, Postgres, A>) -> StoreResult<u64>
    where
        A: IntoArguments<'q, Postgres> + 'q,
    {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::credential::{CredentialForCreate, CredentialProvider},
            error::StoreError,
            meta::StoreId,
            stores::account::AccountStore,
            traits::{contains::FilterByContains, crud::*, join::GetOneToMany},
        },
    };
    use anyhow::Result;
    use modql::filter::{ListOptions, OpValsString};
    use serde_json::json;
    use serial_test::serial;
    use uuid::Uuid;

    #[tokio::test]
    #[serial]
    async fn test_mock_dbx() -> StoreResult<()> {
        let mock_dbx = MockDbx {};

        let sql = r#""#;

        let query = sqlx::query(sql);

        let res = mock_dbx.execute(query).await?;

        assert_eq!(res, 0, "should be zero");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_mock_dbx_with_account_store() -> StoreResult<()> {
        let mock_dbx = MockDbx {};
        let dbx = Arc::new(mock_dbx);
        let account_store = AccountStore::new(dbx);

        let ctx = StoreCtx::bootstrap();

        let id = Uuid::new_v4();

        let res = account_store.get_opt(&ctx, &id.into()).await?;

        assert!(res.is_none(), "should be None");

        Ok(())
    }
}

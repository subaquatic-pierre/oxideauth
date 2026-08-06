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
};

/// PgDbx is a th
#[async_trait]
pub trait DbExecutor: Send + Sync + Unpin {
    /// Execute a `query_as` and fetch exactly one row.
    /// If a transaction is active, runs against it; otherwise uses the pool.
    async fn fetch_one<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<O>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q;

    /// Execute a `query_as` and fetch an optional row.
    /// If a transaction is active, runs against it; otherwise uses the pool.
    async fn fetch_optional<'q, O, A>(
        &self,
        query: QueryAs<'q, Postgres, O, A>,
    ) -> StoreResult<Option<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q;

    /// Execute a `query_as` and fetch all rows.
    /// If a transaction is active, runs against it; otherwise uses the pool.
    async fn fetch_all<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<Vec<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q;

    /// Execute a `query` (no mapping) and return rows affected.
    /// If a transaction is active, runs against it; otherwise uses the pool.
    async fn execute<'q, A>(&self, query: Query<'q, Postgres, A>) -> StoreResult<u64>
    where
        A: IntoArguments<'q, Postgres> + 'q;
}

#[async_trait]
impl<T: DbExecutor> DbExecutor for Arc<T> {
    async fn fetch_one<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<O>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.as_ref().fetch_one(query).await
    }

    async fn fetch_optional<'q, O, A>(
        &self,
        query: QueryAs<'q, Postgres, O, A>,
    ) -> StoreResult<Option<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.as_ref().fetch_optional(query).await
    }

    async fn fetch_all<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<Vec<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.as_ref().fetch_all(query).await
    }

    async fn execute<'q, A>(&self, query: Query<'q, Postgres, A>) -> StoreResult<u64>
    where
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.as_ref().execute(query).await
    }
}

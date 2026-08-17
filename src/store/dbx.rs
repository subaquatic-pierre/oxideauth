use axum::async_trait;
use sqlx::{
    Execute, FromRow, IntoArguments, Postgres, Transaction,
    query::{Query, QueryAs},
};
use std::{
    any::{Any, TypeId},
    collections::{HashMap, VecDeque},
    ops::{Deref, DerefMut},
    sync::{
        Arc, Mutex as SyncMutex,
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
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
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
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
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
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
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
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.fetch_one(query).await
    }

    async fn fetch_optional<'q, O, A>(
        &self,
        query: QueryAs<'q, Postgres, O, A>,
    ) -> StoreResult<Option<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.fetch_optional(query).await
    }

    async fn fetch_all<'q, O, A>(&self, query: QueryAs<'q, Postgres, O, A>) -> StoreResult<Vec<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
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

/// A safe, configurable in-memory [`DbExecutor`] for tests.
///
/// `MockDbx` can be used in two ways:
///
/// * **Empty** (`MockDbx::new()` / `MockDbx::default()`): every method returns a
///   fixed default — `fetch_one` -> `Err(StoreError::MockReturn)`,
///   `fetch_optional` -> `Ok(None)`, `fetch_all` -> `Ok(vec![])`,
///   `execute` -> `Ok(0)`.
/// * **Configured** (via the `with_*` builders): a test registers the exact value
///   each method should return, keyed by the concrete row type `O`. Responses are
///   stored as `Box<dyn Any>` and downcast at call time, so this is a **safe**
///   (no `unsafe`/`transmute`) replacement for the removed
///   `create_dbx_mock_unsafe!` macro.
///
/// Registered responses are consumed in FIFO order: each `with_*` entry is
/// returned exactly once (in registration order for a given type), after which
/// the method falls back to the defaults above.
///
/// ```
#[derive(Default)]
pub struct MockDbx {
    fetch_one_responses: SyncMutex<HashMap<TypeId, VecDeque<Box<dyn Any + Send + Sync>>>>,
    fetch_optional_responses: SyncMutex<HashMap<TypeId, VecDeque<Box<dyn Any + Send + Sync>>>>,
    fetch_all_responses: SyncMutex<HashMap<TypeId, VecDeque<Box<dyn Any + Send + Sync>>>>,
    execute_responses: SyncMutex<VecDeque<StoreResult<u64>>>,
}

impl MockDbx {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the value returned by the next `fetch_one::<T>()` call.
    ///
    /// Multiple registrations of the same type are served in FIFO order.
    pub fn with_one<T: Send + Sync + 'static>(mut self, value: T) -> Self {
        self.fetch_one_responses
            .get_mut()
            .expect("MockDbx fetch_one_responses poisoned")
            .entry(TypeId::of::<T>())
            .or_default()
            .push_back(Box::new(value));
        self
    }

    /// Register the value returned by the next `fetch_optional::<T>()` call.
    ///
    /// Multiple registrations of the same type are served in FIFO order.
    pub fn with_optional<T: Send + Sync + 'static>(mut self, value: Option<T>) -> Self {
        self.fetch_optional_responses
            .get_mut()
            .expect("MockDbx fetch_optional_responses poisoned")
            .entry(TypeId::of::<T>())
            .or_default()
            .push_back(Box::new(value));
        self
    }

    /// Register the value returned by the next `fetch_all::<T>()` call.
    ///
    /// Multiple registrations of the same type are served in FIFO order.
    pub fn with_all<T: Send + Sync + 'static>(mut self, values: Vec<T>) -> Self {
        self.fetch_all_responses
            .get_mut()
            .expect("MockDbx fetch_all_responses poisoned")
            .entry(TypeId::of::<T>())
            .or_default()
            .push_back(Box::new(values));
        self
    }

    /// Register the result returned by the next `execute()` call.
    ///
    /// Multiple registrations are served in FIFO order.
    pub fn with_execute(mut self, result: StoreResult<u64>) -> Self {
        self.execute_responses
            .get_mut()
            .expect("MockDbx execute_responses poisoned")
            .push_back(result);
        self
    }
}

#[async_trait]
impl DbExecutor for MockDbx {
    async fn fetch_one<'q, O, A>(&self, _query: QueryAs<'q, Postgres, O, A>) -> StoreResult<O>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        let entry = self
            .fetch_one_responses
            .lock()
            .expect("MockDbx fetch_one_responses poisoned")
            .get_mut(&TypeId::of::<O>())
            .and_then(VecDeque::pop_front);

        match entry {
            Some(boxed) => match boxed.downcast::<O>() {
                Ok(value) => Ok(*value),
                Err(_) => Err(StoreError::MockReturn),
            },
            None => Err(StoreError::MockReturn),
        }
    }

    async fn fetch_optional<'q, O, A>(
        &self,
        _query: QueryAs<'q, Postgres, O, A>,
    ) -> StoreResult<Option<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        let entry = self
            .fetch_optional_responses
            .lock()
            .expect("MockDbx fetch_optional_responses poisoned")
            .get_mut(&TypeId::of::<O>())
            .and_then(VecDeque::pop_front);

        match entry {
            Some(boxed) => match boxed.downcast::<Option<O>>() {
                Ok(value) => Ok(*value),
                Err(_) => Err(StoreError::MockReturn),
            },
            None => Ok(None),
        }
    }

    async fn fetch_all<'q, O, A>(&self, _query: QueryAs<'q, Postgres, O, A>) -> StoreResult<Vec<O>>
    where
        O: for<'r> FromRow<'r, <Postgres as sqlx::Database>::Row> + Send + Unpin + 'static,
        A: IntoArguments<'q, Postgres> + 'q,
    {
        let entry = self
            .fetch_all_responses
            .lock()
            .expect("MockDbx fetch_all_responses poisoned")
            .get_mut(&TypeId::of::<O>())
            .and_then(VecDeque::pop_front);

        match entry {
            Some(boxed) => match boxed.downcast::<Vec<O>>() {
                Ok(values) => Ok(*values),
                Err(_) => Err(StoreError::MockReturn),
            },
            None => Ok(vec![]),
        }
    }

    async fn execute<'q, A>(&self, _query: Query<'q, Postgres, A>) -> StoreResult<u64>
    where
        A: IntoArguments<'q, Postgres> + 'q,
    {
        self.execute_responses
            .lock()
            .expect("MockDbx execute_responses poisoned")
            .pop_front()
            .unwrap_or(Ok(0))
    }
}


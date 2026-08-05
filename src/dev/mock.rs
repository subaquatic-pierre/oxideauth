/// Creates a mock implementation of `DbExecutor` using `unsafe` code.
///
/// This macro generates the exact code from your template, which uses an `unsafe`
/// block with `mem::transmute_copy`. This will be forbidden by your project's
/// VS Code settings (`-F unsafe-code`). Use `create_dbx_mock_safe!` instead.
///
/// # Usage
///
/// ```rust
/// // In your test:
/// use crate::store::entities::account::AccountRow;
/// use std::mem;
///
/// create_dbx_mock_unsafe!(
///     MockDbxAccountCreateSuccessUnsafe,
///     fetch_one: {
///         let acc = AccountRow::default();
///         let result = unsafe { mem::transmute_copy::<AccountRow, O>(&acc) };
///         mem::forget(acc);
///         Ok(result)
///     },
///     fetch_optional: { Ok(None) },
///     fetch_all: { Ok(vec![]) },
///     execute: { Ok(1) }
/// );
/// ```
#[macro_export]
#[cfg(test)]
macro_rules! create_dbx_mock_unsafe {
    (
        $name:ident,
        fetch_one: { $($fetch_one_body:tt)* },
        fetch_optional: { $($fetch_optional_body:tt)* },
        fetch_all: { $($fetch_all_body:tt)* },
        execute: { $($execute_body:tt)* }
    ) => {
        struct $name;

        #[axum::async_trait]
        impl $crate::store::traits::dbx::DbExecutor for $name {
            async fn fetch_one<'q, O, A>(
                &self,
                _query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, A>,
            ) -> $crate::store::error::StoreResult<O>
            where
                O: for<'r> sqlx::FromRow<'r, <sqlx::Postgres as sqlx::Database>::Row> + Send + Unpin,
                A: sqlx::IntoArguments<'q, sqlx::Postgres> + 'q,
            {
                $($fetch_one_body)*
            }

            async fn fetch_optional<'q, O, A>(
                &self,
                _query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, A>,
            ) -> $crate::store::error::StoreResult<Option<O>>
            where
                O: for<'r> sqlx::FromRow<'r, <sqlx::Postgres as sqlx::Database>::Row> + Send + Unpin,
                A: sqlx::IntoArguments<'q, sqlx::Postgres> + 'q,
            {
                $($fetch_optional_body)*
            }

            async fn fetch_all<'q, O, A>(
                &self,
                _query: sqlx::query::QueryAs<'q, sqlx::Postgres, O, A>,
            ) -> $crate::store::error::StoreResult<Vec<O>>
            where
                O: for<'r> sqlx::FromRow<'r, <sqlx::Postgres as sqlx::Database>::Row> + Send + Unpin,
                A: sqlx::IntoArguments<'q, sqlx::Postgres> + 'q,
            {
                $($fetch_all_body)*
            }

            async fn execute<'q, A>(
                &self,
                _query: sqlx::query::Query<'q, sqlx::Postgres, A>,
            ) -> $crate::store::error::StoreResult<u64>
            where
                A: sqlx::IntoArguments<'q, sqlx::Postgres> + 'q,
            {
                $($execute_body)*
            }
        }
    };
}

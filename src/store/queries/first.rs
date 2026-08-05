use modql::filter::{FilterGroups, ListOptions};
use sea_query::{Asterisk, Condition, PostgresQueryBuilder, Query};
use sea_query::{Expr, Iden};
use sea_query_binder::SqlxBinder;
use sqlx::{postgres::PgRow, FromRow};
use sqlx::{query_as_with, Value};

use crate::store::dbx::PgDbx;
use crate::store::entities::workspace::WorkspaceIden;
use crate::store::error::{StoreError, StoreResult};
use crate::store::queries::meta::ReadQueryMeta;
use crate::store::traits::dbx::DbExecutor;
use crate::store::traits::meta::{StoreRow, TableIden};
use crate::store::{ctx::StoreCtx, manager::StoreManager};
use crate::store::{traits::meta::Store, utils::ListOptionsValidator};

/// Retrieves the first entity that matches the optional filter and options,
/// ensuring a deterministic result by applying a default ordering if none is specified.
///
/// This performs a `SELECT * FROM table WHERE condition ORDER BY ... LIMIT 1` query.
///
/// # Logic
///
/// 1. **Filtering**: Applies the optional `filter`.
/// 2. **Ordering**: If `opts` does not specify `order_bys`, it enforces a default order (e.g., descending by `created_at`) to ensure the "first" result is consistent.
/// 3. **Limiting**: Explicitly sets the query `LIMIT` to 1.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched row (`StoreRow`).
/// * `F`: A type convertible into `FilterGroups`.
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `filter`: An optional filter to apply to the rows.
/// * `opts`: Optional `ListOptions` for ordering (limit/offset are overridden).
/// * `meta`: Metadata about the read query.
///
/// # Returns
///
/// A `StoreResult<Option<T>>` containing:
/// * `Ok(Some(T))`: The first entity found that matches the criteria.
/// * `Ok(None)`: If no rows were found.
/// * `Err(StoreError)`: If the query execution fails.
pub async fn first_opt<E: DbExecutor, T: StoreRow, F: Into<FilterGroups>, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    filter: Option<F>,
    opts: Option<ListOptions>,
    meta: &ReadQueryMeta<I>,
) -> StoreResult<Option<T>> {
    let mut query = Query::select();

    // FROM {DB::TABLE_NAME} SELECT *
    query.from(meta.table).column(Asterisk);

    if let Some(ws_id) = ctx.workspace_scope() {
        let enforced_condition =
            Condition::all().add(Expr::col(WorkspaceIden::WorkspaceId).eq(ws_id));

        query.cond_where(enforced_condition);
    }

    // apply filter
    if let Some(filter) = filter {
        let filters: FilterGroups = filter.into();
        let cond: Condition = filters.try_into()?;
        query.cond_where(cond);
    }

    // validate list options
    let mut list_opts = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;
    // ensure deterministic first if caller didn’t provide order
    if list_opts.order_bys.is_none() {
        // choose your house default:
        // most recent created first
        list_opts.order_bys = Some(vec!["!created_at".to_string()].into());
    }
    list_opts.limit = Some(1);
    list_opts.apply_to_sea_query(&mut query);

    // build sql
    let (sql, vals) = query.build_sqlx(PostgresQueryBuilder);

    // run query, expecting at most one row
    let sqlx = query_as_with::<_, T, _>(&sql, vals);

    let ret = dbx.fetch_optional(sqlx).await?;

    Ok(ret)
}

/// Retrieves the first entity that matches the optional filter and options,
/// and **requires** that at least one entity is found.
///
/// This function calls `first_opt` internally. If no entity is found, it returns
/// a specific `EntityNotFound` error.
///
/// # Type Parameters
///
/// * `T`: The type representing the fetched row (`StoreRow`).
/// * `F`: A type convertible into `FilterGroups`.
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `filter`: An optional filter to apply to the rows.
/// * `opts`: Optional `ListOptions` for ordering (limit/offset are overridden).
/// * `meta`: Metadata about the read query.
///
/// # Returns
///
/// A `StoreResult<T>` containing:
/// * `Ok(T)`: The first entity found that matches the criteria.
/// * `Err(StoreError::EntityNotFound)`: If no row was found.
/// * `Err(StoreError)`: If the underlying query execution fails.
pub async fn first<T: StoreRow, F: Into<FilterGroups>, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &impl DbExecutor,
    filter: Option<F>,
    opts: Option<ListOptions>,
    meta: &ReadQueryMeta<I>,
) -> StoreResult<T> {
    match first_opt(ctx, dbx, filter, opts, meta).await? {
        Some(t) => Ok(t),
        None => Err(StoreError::EntityNotFound {
            entity: meta.table.to_string(),
            id: "first".to_string(),
        }),
    }
}

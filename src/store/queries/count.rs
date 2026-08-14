use modql::filter::{FilterGroups, ListOptions};
use sea_query::{Asterisk, Condition, PostgresQueryBuilder, Query};
use sea_query::{Expr, Func, Iden, ExprTrait};
use crate::store::utils::pg_binder::PgBinder;
use sqlx::Row;
use sqlx::{postgres::PgRow, FromRow};
use sqlx::{query_as_with, query_scalar_with, query_with, Value};

use crate::store::dbx::PgDbx;
use crate::store::entities::workspace::WorkspaceIden;
use crate::store::error::{StoreError, StoreResult};
use crate::store::queries::meta::{
    ContainsFilter, ContainsFilterQueryMeta, CountManyQueryMeta, ReadQueryMeta,
};
use crate::store::traits::dbx::DbExecutor;
use crate::store::traits::meta::{StoreId, TableIden};
use crate::store::{ctx::StoreCtx, manager::StoreManager};
use crate::store::{traits::meta::Store, utils::ListOptionsValidator};

/// Counts the total number of entities (rows) in a table that match an
/// optional set of filters.
///
/// This is a generic function used to determine the size of a filtered result
/// set without fetching all the data.
///
/// # Type Parameters
///
/// * `E`: The database executor trait implementation (`DbExecutor`).
/// * `F`: A type that can be converted into `FilterGroups` (e.g., your `AccountFilter` struct).
/// * `I`: The identifier for the table being queried (`TableIden`).
///
/// # Arguments
///
/// * `_ctx`: The store context (currently unused, denoted by `_`).
/// * `dbx`: The database executor used to run the query.
/// * `filter`: An optional set of filters (`F`) to apply to the count. If `None`,
///   all rows in the table will be counted.
/// * `meta`: Metadata about the read query, containing the target table identifier (`I`).
///
/// # Query Performed (Example)
///
/// If `table` is `account` and a filter for `email` is provided, the SQL generated is:
///
/// ```sql
/// SELECT COUNT(*) AS count FROM account WHERE email = $1
/// ```
///
/// # Returns
///
/// A `StoreResult<i64>` containing:
/// * `Ok(count)`: The total number of rows matching the optional filters.
/// * `Err(StoreError)`: If there is an issue converting the filters or executing the query.
pub async fn count<E: DbExecutor, F: Into<FilterGroups>, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    filter: Option<F>,
    meta: &ReadQueryMeta<I>,
) -> StoreResult<i64> {
    let mut query = Query::select();

    // SELECT COUNT(*)
    query
        .expr_as(Func::count(Expr::col(Asterisk)), "count")
        .from(meta.table);

    if let Some(ws_id) = ctx.workspace_scope() {
        // Add WHERE clause for workspace_id
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    // apply filter
    if let Some(filter) = filter {
        let filters: FilterGroups = filter.into();
        let cond: Condition = filters.try_into()?;
        query.cond_where(cond);
    }

    // build SQL and values
    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_as_with(&sql, vals);
    let count: (i64,) = dbx.fetch_one(query).await?;

    Ok(count.0)
}

/// Counts the number of related entities in a "many" (or child) table
/// based on a foreign key relationship to a single "one" (or parent) entity ID.
///
/// This is typically used for calculating the number of child records associated
/// with a parent record (e.g., counting the number of comments for a single post).
///
/// # Type Parameters
///
/// * `E`: The database executor trait implementation (`DbExecutor`).
/// * `I`: The identifier for the table being counted (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context, often used for authorization or transaction handling.
/// * `dbx`: The database executor used to run the query.
/// * `id`: The unique identifier (`StoreId`) of the parent entity. This value
///   is used to filter the child table on the foreign key column.
/// * `meta`: Metadata about the count query, including:
///     * `table`: The identifier of the many (child) table being counted.
///     * `fk`: The name of the foreign key column in the child table that links
///       back to the parent entity.
///
/// # Query Performed (Example)
///
/// If `table` is `comments` and `fk` is `post_id`, the SQL generated is:
///
/// ```sql
/// SELECT COUNT(*) FROM comments WHERE post_id = $1
/// ```
///
/// # Returns
///
/// A `StoreResult<i64>` containing:
/// * `Ok(count)`: The total number of rows in the child table matching the
///   provided foreign key `id`.
/// * `Err(StoreError)`: If the query fails to execute.
pub async fn count_many<E: DbExecutor, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    id: &impl StoreId,
    meta: &CountManyQueryMeta<I>,
) -> StoreResult<i64> {
    let mut query = Query::select();

    if let Some(ws_id) = ctx.workspace_scope() {
        // Add WHERE clause for workspace_id
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    // SELECT COUNT(*) FROM {many_table}
    query
        .expr(Func::count(Expr::col(Asterisk)))
        .from(meta.table)
        .and_where(Expr::col(meta.fk).eq(Expr::val(id.clone()))); // WHERE foreign_key = ?

    let (sql, values) = query.build(PostgresQueryBuilder);
    let values = PgBinder(values.0);

    // Here we can't use `try_get("count")` as easily without an alias,
    // so we fetch into a tuple, which is very efficient.
    let query = sqlx::query_as_with(&sql, values);
    let count: (i64,) = dbx.fetch_one(query).await?;

    Ok(count.0)
}

/// Counts the number of entities that satisfy a 'contains' condition on a column.
///
/// This function is generic over the DbExecutor and the TableIden.
///
/// # Arguments
/// * `_ctx` - The store context (ignored for simplicity here).
/// * `dbx` - The database executor.
/// * `value` - The value to check for containment (Array of Strings or JSON).
/// * `meta` - Metadata including the table and the target column name.
///
/// # Returns
/// A StoreResult containing the count (i64).
pub async fn count_contains<E, I>(
    ctx: &StoreCtx,
    dbx: &E,
    value: ContainsFilter,
    meta: &ContainsFilterQueryMeta<I>,
) -> StoreResult<i64>
where
    E: DbExecutor,
    I: TableIden,
{
    let mut query = Query::select();

    if let Some(ws_id) = ctx.workspace_scope() {
        // Add WHERE clause for workspace_id
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    // SELECT COUNT(*)
    query
        .expr_as(Func::count(Expr::col(Asterisk)), "count")
        .from(meta.table);

    // Build the expression for the containment check (@>)
    let expr = match value {
        // For Array: "col_name" @> $1
        ContainsFilter::Array(tags) => {
            Expr::cust_with_values(format!(r#""{}" @> $"#, meta.col.to_string()), [tags])
        }
        // For JSON: "col_name" @> $1 (checks for key/value containment in JSONB)
        ContainsFilter::Json(json) => {
            Expr::cust_with_values(format!(r#""{}" @> $"#, meta.col.to_string()), [json])
        }
    };

    // Apply the containment expression as a WHERE clause
    query.cond_where(expr);

    // build SQL and values
    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    // Execute query
    let sqlx_query = sqlx::query_as_with(&sql, vals);

    // fetch_one returns a tuple (i64,) for COUNT(*)
    let count: (i64,) = dbx.fetch_one(sqlx_query).await?;

    Ok(count.0)
}

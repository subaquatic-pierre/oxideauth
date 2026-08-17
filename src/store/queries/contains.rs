use crate::store::utils::{apply_tags_to_query, pg_binder::PgBinder};
use modql::filter::{FilterGroups, ListOptions};
use sea_query::{Asterisk, Condition, Expr, ExprTrait, Func, PostgresQueryBuilder, Query};
use serde_json::Value as JsonValue;
use tracing::debug;

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::workspace::WorkspaceIden,
    error::{StoreError, StoreResult},
    queries::meta::{ContainsFilter, ContainsFilterQueryMeta},
    traits::{
        dbx::DbExecutor,
        meta::{StoreRow, TableIden},
    },
    utils::ListOptionsValidator,
};

/// Fetches all entities (rows) from a table where a specified column contains
/// the provided value. This function utilizes PostgreSQL's containment operator (`@>`),
/// which is typically used for querying array or JSONB columns.
///
/// # Note on Limit Checking
///
/// The comment `// count ensure list limit not exceeded` suggests an upstream requirement
/// to check limits before fetching. This method focuses on executing the filtered
/// selection query itself.
///
/// # Type Parameters
///
/// * `E`: The database executor trait implementation (`DbExecutor`).
/// * `T`: The type representing the fetched row (must implement `StoreRow`).
/// * `I`: The identifier for the table being queried (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context (currently unused in the provided implementation).
/// * `dbx`: The database executor used to run the query.
/// * `value`: The containment value, wrapped in `ContainsFilter`:
///     * `ContainsFilter::Array(Vec<String>)`: Used for array containment checks (e.g., checking if an array column contains all given strings).
///     * `ContainsFilter::Json(Value)`: Used for JSONB containment checks (e.g., checking if a JSONB column contains a specific key/value subset).
/// * `meta`: Metadata about the containment query, including:
///     * `table`: The identifier of the table being queried.
///     * `col`: The name of the column on which the containment check is performed.
///
/// # Query Performed (Example)
///
/// If `table` is `post`, `col` is `tags`, and `value` is `ContainsFilter::Array(["rust", "async"])`,
/// the core SQL condition generated is:
///
/// ```sql
/// ... WHERE "tags" @> $1
/// ```
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing:
/// * `Ok(Vec<T>)`: A vector of entities (rows) that satisfy the containment condition.
/// * `Err(StoreError)`: If the query fails to execute.
pub async fn filter_by_value_contains<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    value: ContainsFilter,
    opts: Option<ListOptions>,
    meta: &ContainsFilterQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    // count ensure list limit not exceeded

    let mut query = Query::select();
    query.from(meta.table).column((meta.table, Asterisk));

    let expr = match value {
        ContainsFilter::Array(tags) => {
            Expr::cust_with_values(format!(r#""{}" @> $"#, meta.col.to_string()), [tags])
        }
        ContainsFilter::Json(json) => {
            Expr::cust_with_values(format!(r#""{}" @> $"#, meta.col.to_string()), [json])
        }
    };

    query.and_where(expr);

    if let Some(ws_id) = ctx.workspace_scope() {
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    // validate list options
    // Assuming ListOptionsValidator and has_audit exist in scope/meta for the filter
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;

    // add list options to query (e.g., limit, offset, order_by)
    list_options.apply_to_sea_query(&mut query);

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_as_with(&sql, vals);

    let res = dbx.fetch_all(query).await?;

    Ok(res)
}

/// Fetches entities where the designated array column contains the given tags
/// AND the row also matches the optional field-based filter.
///
/// This combines PostgreSQL's `@>` containment operator with standard
/// `FilterGroups` conditions in a single SQL query.
///
/// If both `tags` and `filter` are `None`, this acts as a simple list-all query.
pub async fn list_with_contains<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    tags: Option<Vec<String>>,
    filter: Option<FilterGroups>,
    opts: Option<ListOptions>,
    meta: &ContainsFilterQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    let mut query = Query::select();
    query.from(meta.table).column((meta.table, Asterisk));

    // --- Tags containment (@>) ---
    if let Some(tags) = tags {
        apply_tags_to_query(&mut query, meta.table, tags);
    }

    // --- Field-based filter ---
    if let Some(filter) = filter {
        let cond: Condition = filter.try_into()?;
        query.cond_where(cond);
    }

    // --- Workspace scoping ---
    if let Some(ws_id) = ctx.workspace_scope() {
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    // --- List options (limit/offset/order) ---
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;
    list_options.apply_to_sea_query(&mut query);

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    // debug!("SQL: {:#?}, VALS: {:#?}", sql, vals);

    let query = sqlx::query_as_with(&sql, vals);
    let res = dbx.fetch_all(query).await?;

    Ok(res)
}

/// Returns the count of rows where the designated array column contains the given
/// tags AND the row also matches the optional field-based filter.
pub async fn count_with_contains<E: DbExecutor, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    tags: Option<Vec<String>>,
    filter: Option<FilterGroups>,
    meta: &ContainsFilterQueryMeta<I>,
) -> StoreResult<i64>
where
    E: DbExecutor,
    I: TableIden,
{
    let mut query = Query::select();

    // --- Workspace scoping ---
    if let Some(ws_id) = ctx.workspace_scope() {
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    query
        .expr_as(Func::count(Expr::col(Asterisk)), "count")
        .from(meta.table);

    // --- Tags containment (@>) ---
    if let Some(tags) = tags {
        apply_tags_to_query(&mut query, meta.table, tags);
    }

    // --- Field-based filter ---
    if let Some(filter) = filter {
        let cond: Condition = filter.try_into()?;
        query.cond_where(cond);
    }

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);
    let sqlx_query = sqlx::query_as_with::<_, (i64,), _>(&sql, vals);

    let (count,) = dbx.fetch_one(sqlx_query).await?;
    Ok(count)
}

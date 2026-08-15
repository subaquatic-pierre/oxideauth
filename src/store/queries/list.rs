use modql::filter::{FilterGroups, ListOptions};
use sea_query::{
    Asterisk, Condition, Expr, ExprTrait, Func, JoinType, PostgresQueryBuilder, Query,
};

use crate::store::{
    ctx::StoreCtx,
    entities::workspace::WorkspaceIden,
    error::{StoreError, StoreResult},
    queries::meta::{ListContainingManyQueryMeta, ListInNamespaceQueryMeta},
    traits::{
        dbx::DbExecutor,
        meta::{StoreId, StoreRow, TableIden},
    },
    utils::{ListOptionsValidator, PgBinder, apply_tags_to_query},
};

/// Lists entities (e.g., accounts) that belong to the current namespace by
/// joining on a membership/join table.
///
/// The listed table (e.g., `account`) has no `workspace_id` of its own, so the
/// namespace boundary is enforced on the join table's `workspace_id` column.
///
/// This performs a `SELECT DISTINCT {table}.* FROM {table} INNER JOIN {join_table} ...`
/// query; `DISTINCT` guards against duplicate rows when a single entity is linked
/// to the namespace through multiple join-table rows (e.g., one workspace-scoped
/// membership plus several project-scoped memberships).
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched rows (`StoreRow`).
/// * `F`: A type convertible into `FilterGroups` for filtering the listed table.
/// * `I`: The table identifier of the listed table (`TableIden`).
/// * `J`: The table identifier of the join table (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context. The namespace is resolved from `ctx.workspace_scope()`
///   when set, otherwise falling back to the context's operational workspace (`ctx.ws_id`).
/// * `dbx`: The database executor.
/// * `filter`: An optional filter applied to the listed table.
/// * `opts`: Optional `ListOptions` for sorting and pagination.
/// * `meta`: Metadata describing the listed table, join table, and their key columns.
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing the entities found in the namespace.
pub async fn list_in_namespace_by_join_table<E, T, F, I>(
    ctx: &StoreCtx,
    dbx: &E,
    tags: Option<Vec<String>>,
    filter: Option<F>,
    opts: Option<ListOptions>,
    meta: &ListInNamespaceQueryMeta<I>,
) -> StoreResult<Vec<T>>
where
    E: DbExecutor,
    T: StoreRow,
    F: Into<FilterGroups>,
    I: TableIden,
{
    let namespace = ctx.workspace_scope().ok_or(StoreError::InvalidContext(
        "workspace needs to be defined in `list_in_namespace_by_join_table` query".to_string(),
    ))?;

    let mut query = Query::select();
    query
        .from(meta.table)
        .distinct()
        .column((meta.table, Asterisk))
        .join(
            JoinType::InnerJoin,
            meta.join_table,
            Expr::col((meta.table, meta.pk)).equals((meta.join_table, meta.join_fk)),
        )
        .and_where(
            Expr::col((meta.join_table, WorkspaceIden::WorkspaceId)).eq(Expr::val(namespace)),
        );

    // --- Tags containment (@>) ---
    if let Some(tags) = tags {
        apply_tags_to_query(&mut query, meta.table, tags);
    }

    // Apply user filters against the listed table.
    if let Some(filter) = filter {
        let filters: FilterGroups = filter.into();
        let cond: Condition = filters.try_into()?;
        query.cond_where(cond);
    }

    // Validate list options and apply ordering/limit/offset.
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;
    list_options.apply_to_sea_query(&mut query);

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);
    let sqlx = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let ret = dbx.fetch_all(sqlx).await?;

    Ok(ret)
}

/// Lists entities (e.g., roles, memberships) whose set of linked records
/// (via a join table) **contains all** of the provided IDs.
///
/// This performs a `SELECT {table}.* FROM {table} INNER JOIN {join_table} ...
/// WHERE {join_many_fk} IN (...) GROUP BY {pk} HAVING COUNT({join_many_fk}) = n`
/// query, where `n` is the number of distinct requested IDs. The `GROUP BY` +
/// `HAVING` combination guarantees only entities linked to **every** requested
/// ID are returned.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched rows (`StoreRow`).
/// * `F`: A type convertible into `FilterGroups` for filtering the listed table.
/// * `I`: The table identifier (`TableIden`).
/// * `ID`: The primary key type of the "many" entities (`StoreId`, e.g., `DbId`).
///
/// # Arguments
///
/// * `ctx`: The store context, used for workspace scoping.
/// * `dbx`: The database executor.
/// * `ids`: The IDs the entity's linked set must contain (all of them).
/// * `filter`: An optional filter applied to the listed table.
/// * `opts`: Optional `ListOptions` for sorting and pagination.
/// * `meta`: Metadata describing the listed table, join table, and their key columns.
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing the entities whose linked set contains all
/// of the requested IDs.
pub async fn list_containing_many<E, T, F, I>(
    ctx: &StoreCtx,
    dbx: &E,
    ids: Vec<impl StoreId>,
    tags: Option<Vec<String>>,
    filter: Option<F>,
    opts: Option<ListOptions>,
    meta: &ListContainingManyQueryMeta<I>,
) -> StoreResult<Vec<T>>
where
    E: DbExecutor,
    T: StoreRow,
    F: Into<FilterGroups>,
    I: TableIden,
{
    // Nothing to match against: an empty "must contain all" set matches no rows.
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // Deduplicate the requested IDs. The HAVING clause counts distinct join rows,
    // so duplicate input would otherwise skew the count comparison.
    // NOTE: the IDs are kept in their native type (e.g., `DbId` -> `uuid` column)
    // so the generated `... IN (...)` predicate binds uuid values. Binding the
    // IDs as `String` (text) would make PostgreSQL reject the query with
    // "operator does not exist: uuid = text" on uuid-typed join columns.
    let mut unique: Vec<_> = Vec::with_capacity(ids.len());
    {
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            if seen.insert(id.to_string()) {
                unique.push(id);
            }
        }
    }
    let n = unique.len() as i64;

    let mut query = Query::select();
    query
        .from(meta.table)
        .column((meta.table, Asterisk))
        .join(
            JoinType::InnerJoin,
            meta.join_table,
            Expr::col((meta.join_table, meta.join_fk)).equals((meta.table, meta.pk)),
        )
        .and_where(Expr::col((meta.join_table, meta.join_many_fk)).is_in(unique))
        .group_by_col((meta.table, meta.pk))
        .and_having(Func::count(Expr::col((meta.join_table, meta.join_many_fk))).eq(Expr::val(n)));

    // Enforce workspace scoping on the listed table.
    if let Some(ws_id) = ctx.workspace_scope() {
        query.and_where(Expr::col((meta.table, WorkspaceIden::WorkspaceId)).eq(Expr::val(ws_id)));
    }

    // --- Tags containment (@>) ---
    if let Some(tags) = tags {
        apply_tags_to_query(&mut query, meta.table, tags);
    }

    // Apply user filters against the listed table.
    if let Some(filter) = filter {
        let filters: FilterGroups = filter.into();
        let cond: Condition = filters.try_into()?;
        query.cond_where(cond);
    }

    // Validate list options and apply ordering/limit/offset.
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;
    list_options.apply_to_sea_query(&mut query);

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);
    let sqlx = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let ret = dbx.fetch_all(sqlx).await?;

    Ok(ret)
}

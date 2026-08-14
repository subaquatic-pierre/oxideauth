use crate::store::utils::pg_binder::PgBinder;
use modql::field::HasSeaFields;
use modql::filter::{FilterGroups, ListOptions};
use sea_query::{
    Alias, Asterisk, CaseStatement, Condition, Expr, IntoValueTuple, PostgresQueryBuilder, Query,
    SeaRc, SimpleExpr, WithQuery,
};
use sea_query::{ExprTrait, Iden, IntoIden, TableRef};
use sqlx::{FromRow, postgres::PgRow};
use sqlx::{Postgres, QueryBuilder, Value, query_as_with};
use uuid::Uuid;

use crate::store::dbx::PgDbx;
use crate::store::entities::workspace::WorkspaceIden;
use crate::store::error::{StoreError, StoreResult};
use crate::store::queries::meta::{FindManyWhereValueInKeyMeta, MutateQueryMeta, ReadQueryMeta};
use crate::store::traits::dbx::DbExecutor;
use crate::store::traits::meta::{Store, StoreId, StoreRow, TableIden};
use crate::store::utils::{ListOptionsValidator, prepare_workspace_scope};
use crate::store::utils::{pg_type_of, prepare_audit_fields, push_sq_value};
use crate::store::{ctx::StoreCtx, manager::StoreManager};

/// Inserts multiple new entities into the database in a single batch operation
/// and returns the created entities (including generated IDs, audit fields, etc.).
///
/// This function constructs a single `INSERT INTO ... VALUES (...), (...), ...` query
/// and executes it against the database executor.
///
/// # Type Parameters
///
/// * `E`: The database executor trait implementation (`DbExecutor`).
/// * `T`: The type representing the fetched and created row (must implement `StoreRow`).
/// * `D`: The data transfer object (DTO) for creation (must implement `HasSeaFields` to provide column values).
/// * `I`: The identifier for the table being mutated (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context, providing necessary information like the `user_id` for audit fields.
/// * `dbx`: The database executor used to run the query.
/// * `data`: A `Vec<D>` containing the creation DTOs for the entities to be inserted.
/// * `meta`: Metadata about the mutation query, including the target table identifier (`I`)
///   and a flag indicating if audit fields should be applied (`has_audit`).
///
/// # Logic Flow
///
/// 1. **Checks**: Performs an early exit if `data` is empty and validates the input limit.
/// 2. **Audit**: Prepares audit fields (`created_by`, `created_at`) for each entity if `meta.has_audit` is true.
/// 3. **Query Build**: Iterates through the `data`, collects column names once (from the first item), and adds values for all items.
/// 4. **Return**: The `RETURNING *` clause (`.returning_all()`) ensures the newly created entities are fetched back.
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing:
/// * `Ok(Vec<T>)`: A vector of the fully created and returned entities.
/// * `Err(StoreError)`: If the limit check fails or the query execution encounters an error.
pub async fn create_many<E: DbExecutor, T: StoreRow, D: HasSeaFields, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    data: Vec<D>,
    meta: &MutateQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    // --- Early exit: nothing to do, return empty result.
    if data.is_empty() {
        return Ok(vec![]);
    }

    ListOptionsValidator::validate_limit(data.len() as i64)?;

    let user_id = ctx.user_id;
    let mut query = Query::insert();

    query.into_table(meta.table);

    // flag to only set columns for the first item
    // do not add columns again for any more items
    let mut is_first = true;

    for el in data {
        let mut fields = el.not_none_sea_fields();

        if meta.has_audit {
            prepare_audit_fields(&mut fields, user_id, true);
        }

        let fields = prepare_workspace_scope(fields, ctx.workspace_scope());

        let (cols, vals) = fields.for_sea_insert();

        // add columns if not already added, .ie first iteration
        if is_first {
            query.columns(cols);
            // update is_first to skip for all next iterations
            is_first = false;
        }

        query.values(vals)?;
    }

    query.returning_all();

    let (sql, values) = query.build(PostgresQueryBuilder);
    let values = PgBinder(values.0);
    let sqlx_query = query_as_with::<_, T, _>(&sql, values);

    let ret = dbx.fetch_all(sqlx_query).await?;

    Ok(ret)
}

/// Updates multiple entities in the database, where each entity has a specific ID
/// and a set of fields to update.
///
/// This performs an individual `UPDATE` query for **each** item in the `data` vector.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the returned row (`StoreRow`).
/// * `D`: The data transfer object (DTO) with fields to update (`HasSeaFields`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context, providing the `user_id` for audit fields.
/// * `dbx`: The database executor.
/// * `data`: A vector of tuples `(ID, Updates)`, where `ID` is the primary key
///   and `Updates` is the DTO containing the fields to modify.
/// * `meta`: Metadata including the target table, primary key (`pk`), and audit flag.
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing a vector of the fully updated entities
/// that were successfully found and modified.
pub async fn update_many<E: DbExecutor, T: StoreRow, D: HasSeaFields, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    data: Vec<(impl StoreId, D)>,
    meta: &MutateQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    // exit: nothing to do, return empty result.
    if data.is_empty() {
        return Ok(vec![]);
    }

    // validate list options, ie. max limit
    ListOptionsValidator::validate_limit(data.len() as i64)?;

    let mut updated_rows = Vec::with_capacity(data.len());

    for (id, updates) in data {
        let mut query = Query::update();

        let mut fields = updates.not_none_sea_fields();

        if meta.has_audit {
            prepare_audit_fields(&mut fields, ctx.user_id, false);
        }

        let fields = prepare_workspace_scope(fields, ctx.workspace_scope());

        let fields = fields.for_sea_update();

        let query = query
            .table(meta.table)
            .values(fields)
            .and_where(Expr::col(meta.pk).eq(Expr::val(id.clone())))
            .returning_all();

        let (sql, vals) = query.build(PostgresQueryBuilder);
        let vals = PgBinder(vals.0);

        let query = sqlx::query_as_with::<_, T, _>(&sql, vals);

        let res = dbx.fetch_optional(query).await?;

        if let Some(ret) = res {
            updated_rows.push(ret);
        }
    }

    Ok(updated_rows)
}

/// Deletes multiple entities from the database based on a list of primary keys (IDs).
///
/// This performs a single batch `DELETE FROM ... WHERE pk IN ($1, $2, ...)` query.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the deleted row, which is returned by `RETURNING *` (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context (currently unused in the deletion logic).
/// * `dbx`: The database executor.
/// * `ids`: A vector of primary key identifiers (`StoreId`) of the entities to be deleted.
/// * `meta`: Metadata including the target table and the primary key column name (`pk`).
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing a vector of the fully deleted entities
/// as returned by the `RETURNING ALL` clause.
pub async fn delete_many<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    ids: Vec<impl StoreId>,
    meta: &MutateQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    // --- Early exit: nothing to do, return empty result.
    if ids.is_empty() {
        return Ok(vec![]);
    }

    ListOptionsValidator::validate_limit(ids.len() as i64)?;

    let mut query = Query::delete();

    if let Some(ws_id) = ctx.workspace_scope() {
        // Add WHERE clause for workspace_id
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    query
        .from_table(meta.table)
        .and_where(Expr::col(meta.pk).is_in(ids))
        .returning_all();

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let sqlx = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let ret = dbx.fetch_all(sqlx).await?;

    Ok(ret)
}

pub async fn find_many_where_value_in_key<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    values: Vec<String>,
    meta: &ReadQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = Query::select();

    query
        .from(meta.table)
        .column(Asterisk)
        .and_where(Expr::col((meta.table, meta.pk)).is_in(values));

    if let Some(ws_id) = ctx.workspace_scope() {
        query.and_where(Expr::col((meta.table, WorkspaceIden::WorkspaceId)).eq(Expr::val(ws_id)));
    }

    let (sql, values) = query.build(PostgresQueryBuilder);
    let values = PgBinder(values.0);

    let sqlx = sqlx::query_as_with::<_, T, _>(&sql, values);

    let ret = dbx.fetch_all(sqlx).await?;

    Ok(ret)
}

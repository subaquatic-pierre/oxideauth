use crate::store::utils::pg_binder::PgBinder;
use modql::field::HasSeaFields;
use modql::filter::{FilterGroups, ListOptions};
use sea_query::{Alias, Asterisk, Condition, Expr, IntoValueTuple, PostgresQueryBuilder, Query};
use sea_query::{ExprTrait, Iden, IntoIden, TableRef};
use sqlx::{FromRow, postgres::PgRow};
use sqlx::{Value, query_as_with};
use uuid::Uuid;

use crate::store::dbx::PgDbx;
use crate::store::entities::workspace::WorkspaceIden;
use crate::store::error::{StoreError, StoreResult};
use crate::store::queries::meta::{MutateQueryMeta, ReadQueryMeta};
use crate::store::traits::dbx::DbExecutor;
use crate::store::traits::meta::{Store, StoreId, StoreRow, TableIden};
use crate::store::utils::ListOptionsValidator;
use crate::store::utils::{prepare_audit_fields, prepare_workspace_scope};
use crate::store::{ctx::StoreCtx, manager::StoreManager};

/// Inserts a single new entity into the database and returns the fully created row.
///
/// This performs an `INSERT INTO ... VALUES (...) RETURNING *` query.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched and created row (`StoreRow`).
/// * `D`: The data transfer object (DTO) for creation (`HasSeaFields`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context, providing the required `user_id` for audit fields.
/// * `dbx`: The database executor.
/// * `data`: The creation DTO containing the fields for the new entity.
/// * `meta`: Metadata including the target table and a flag for audit field management (`has_audit`).
///
/// # Returns
///
/// A `StoreResult<T>` containing:
/// * `Ok(T)`: The fully created entity, including the generated primary key and audit timestamps.
/// * `Err(StoreError)`: If the query fails to execute.
pub async fn create<E: DbExecutor, T: StoreRow, D: HasSeaFields, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    data: D,
    meta: &MutateQueryMeta<I>,
) -> StoreResult<T> {
    let user_id = ctx.user_id;
    let mut fields = data.not_none_sea_fields();

    if meta.has_audit {
        prepare_audit_fields(&mut fields, user_id, true);
    }

    // This uses the "Consumes and Returns" method, which is the standard Rust way
    // to perform a mutable transformation when the inner data is private.
    // It is functionally equivalent to an in-place modification of the `fields` variable.
    let fields = prepare_workspace_scope(fields, ctx.workspace_scope());

    let (cols, vals) = fields.for_sea_insert();
    let mut query = Query::insert();
    query
        .into_table(meta.table)
        .columns(cols)
        .values(vals)?
        .returning_all();

    let (sql, values) = query.build(PostgresQueryBuilder);
    let values = PgBinder(values.0);
    let sqlx_query = query_as_with::<_, T, _>(&sql, values);

    let ret = dbx.fetch_one(sqlx_query).await?;

    Ok(ret)
}

/// Retrieves a single entity (row) from the database by its primary key ID,
/// returning `Some(entity)` if found, or `None` if no entity matches the ID.
///
/// This performs a `SELECT * FROM table WHERE pk = $1` query.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched row (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context (currently unused).
/// * `dbx`: The database executor.
/// * `id`: The primary key identifier (`StoreId`) of the entity to retrieve.
/// * `meta`: Metadata including the target table and the primary key column name (`pk`).
///
/// # Returns
///
/// A `StoreResult<Option<T>>` containing:
/// * `Ok(Some(T))`: The entity if a row was found.
/// * `Ok(None)`: If no row was found matching the `id`.
/// * `Err(StoreError)`: If the query execution encounters a database error.
pub async fn get_opt<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    id: &impl StoreId,
    meta: &ReadQueryMeta<I>,
) -> StoreResult<Option<T>> {
    let mut query = Query::select();

    if let Some(ws_id) = ctx.workspace_scope() {
        // Add WHERE clause for workspace_id
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    query
        .from(meta.table)
        .column(Asterisk)
        .and_where(Expr::col(meta.pk).eq(Expr::val(id.clone())));

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);
    let sqlx_query = query_as_with::<_, T, _>(&sql, vals);

    let ret = dbx.fetch_optional(sqlx_query).await?;

    Ok(ret)
}

/// Retrieves a single entity (row) from the database by its primary key ID.
///
/// This function calls `get_opt` internally and **requires** that an entity
/// is found. If no entity matches the ID, it returns a specific `EntityNotFound` error.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched row (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `id`: The primary key identifier (`StoreId`) of the entity to retrieve.
/// * `meta`: Metadata including the target table and the primary key column name.
///
/// # Returns
///
/// A `StoreResult<T>` containing:
/// * `Ok(T)`: The entity if a row was successfully found.
/// * `Err(StoreError::EntityNotFound)`: If no row was found matching the `id`.
/// * `Err(StoreError)`: If the underlying query execution encounters a database error.
pub async fn get<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    id: &impl StoreId,
    meta: &ReadQueryMeta<I>,
) -> StoreResult<T> {
    match get_opt(ctx, dbx, id, meta).await? {
        Some(t) => Ok(t),
        None => Err(StoreError::EntityNotFound {
            entity: meta.table.to_string(),
            id: id.to_string(),
        }),
    }
}

/// Retrieves a list of entities (rows) from a table, applying optional filtering,
/// sorting, and pagination (limit/offset) options.
///
/// This constructs a `SELECT * FROM table WHERE condition ORDER BY ... LIMIT ... OFFSET ...` query.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched rows (`StoreRow`).
/// * `F`: A type convertible into `FilterGroups` for complex filtering.
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `filter`: An optional set of filters to narrow the result set.
/// * `opts`: An optional struct containing pagination (`limit`, `offset`) and sorting (`order_bys`) parameters.
/// * `meta`: Metadata about the read query, including the target table and audit flag.
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing:
/// * `Ok(Vec<T>)`: A vector of entities matching the criteria, potentially paginated.
/// * `Err(StoreError)`: If filter conversion or query execution fails, or if list options validation fails.
pub async fn list<E: DbExecutor, T: StoreRow, F: Into<FilterGroups>, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    filter: Option<F>,
    opts: Option<ListOptions>,
    meta: &ReadQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    let mut query = Query::select();

    // FROM {DB::TABLE_NAME} SELECT *
    query.column(Asterisk).from(meta.table);

    // 1. Context Scoping (The Security Guardrail)
    if let Some(ws_id) = ctx.workspace_scope() {
        // SCENARIO 1: CONTEXT IS SCOPED (Standard User Token)
        // Enforce the workspace boundary unconditionally. This condition is added
        // first and will be ANDed with the user's explicit filter (Step 2).

        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    // 2. Apply User Filters
    // The user's filters (from the handler FilterParam) are applied. Due to sea-query's
    // internal logic, this condition is always ANDed with any preceding
    // cond_where clauses (like the context scope from Step 1).
    if let Some(filter) = filter {
        let filters: FilterGroups = filter.into();
        let cond: Condition = filters.try_into()?;
        query.cond_where(cond);
    }

    // validate list options
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;
    // add list options to query, there will always at least be maximum limit
    list_options.apply_to_sea_query(&mut query);

    // build sql
    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    // build sqlx query
    let sqlx = query_as_with::<_, T, _>(&sql, vals);

    // execute query against dbx
    let ret = dbx.fetch_all(sqlx).await?;

    Ok(ret)
}

/// Updates an existing entity in the database by its primary key ID with the provided data.
///
/// Returns `Some(T)` with the updated entity if the row was found and modified, or `None` if
/// no entity matching the ID was found.
///
/// This performs an `UPDATE table SET ... WHERE pk = $1 RETURNING *` query.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched, updated row (`StoreRow`).
/// * `D`: The data transfer object (DTO) containing fields to update (`HasSeaFields`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context, providing the `user_id` for setting audit fields (`updated_by`, `updated_at`).
/// * `dbx`: The database executor.
/// * `id`: The primary key identifier (`StoreId`) of the entity to update.
/// * `data`: The DTO with the fields to modify. Only non-None fields are included in the update.
/// * `meta`: Metadata including the target table, primary key (`pk`), and audit flag.
///
/// # Returns
///
/// A `StoreResult<Option<T>>` containing:
/// * `Ok(Some(T))`: The fully updated entity as returned by `RETURNING ALL`.
/// * `Ok(None)`: If no entity was found matching the primary key `id`.
/// * `Err(StoreError)`: If the query execution encounters a database error.
pub async fn update_opt<E: DbExecutor, T: StoreRow, D: HasSeaFields, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    id: &impl StoreId,
    data: D,
    meta: &MutateQueryMeta<I>,
) -> StoreResult<Option<T>> {
    let mut query = Query::update();
    let user_id = ctx.user_id.to_string();

    let mut fields = data.not_none_sea_fields();

    if meta.has_audit {
        prepare_audit_fields(&mut fields, ctx.user_id, false);
    }

    let fields = fields.for_sea_update();

    if let Some(ws_id) = ctx.workspace_scope() {
        // Add WHERE clause for workspace_id
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    let query = query
        .table(meta.table)
        .values(fields)
        .and_where(Expr::col(meta.pk).eq(Expr::val(id.clone())))
        .returning_all();

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let sqlx = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let ret = dbx.fetch_optional(sqlx).await?;

    Ok(ret)
}

/// Updates an existing entity in the database by its primary key ID and **requires**
/// that the entity is found and successfully updated.
///
/// This function calls `update_opt` internally and returns a specific `EntityNotFound`
/// error if no entity matching the ID is found.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the fetched, updated row (`StoreRow`).
/// * `D`: The data transfer object (DTO) containing fields to update (`HasSeaFields`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context, providing the `user_id` for audit fields.
/// * `dbx`: The database executor.
/// * `id`: The primary key identifier (`StoreId`) of the entity to update.
/// * `data`: The DTO with the fields to modify.
/// * `meta`: Metadata including the target table, primary key, and audit flag.
///
/// # Returns
///
/// A `StoreResult<T>` containing:
/// * `Ok(T)`: The fully updated entity.
/// * `Err(StoreError::EntityNotFound)`: If no row was found matching the `id`.
/// * `Err(StoreError)`: If the underlying query execution fails.
pub async fn update<E: DbExecutor, T: StoreRow, D: HasSeaFields, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    id: &impl StoreId,
    data: D,
    meta: &MutateQueryMeta<I>,
) -> StoreResult<T> {
    match update_opt(ctx, dbx, id, data, meta).await? {
        Some(t) => Ok(t),
        None => Err(StoreError::EntityNotFound {
            entity: meta.table.to_string(),
            id: id.to_string(),
        }),
    }
}

/// Deletes a single entity from the database by its primary key ID.
///
/// Returns `Some(T)` with the deleted entity if the row was found and removed, or `None` if
/// no entity matching the ID was found.
///
/// This performs a `DELETE FROM table WHERE pk = $1 RETURNING *` query.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the deleted row (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context (currently unused in the deletion logic).
/// * `dbx`: The database executor.
/// * `id`: The primary key identifier (`StoreId`) of the entity to delete.
/// * `meta`: Metadata including the target table and the primary key column name (`pk`).
///
/// # Returns
///
/// A `StoreResult<Option<T>>` containing:
/// * `Ok(Some(T))`: The deleted entity as returned by `RETURNING ALL`.
/// * `Ok(None)`: If no entity was found matching the primary key `id`.
/// * `Err(StoreError)`: If the query execution encounters a database error.
pub async fn delete_opt<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    id: &impl StoreId,
    meta: &MutateQueryMeta<I>,
) -> StoreResult<Option<T>> {
    let id_str = id.to_string();
    let mut query = Query::delete();

    if let Some(ws_id) = ctx.workspace_scope() {
        // Add WHERE clause for workspace_id
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    query
        .from_table(meta.table)
        .and_where(Expr::col(meta.pk).eq(Expr::val(id.clone())))
        .returning_all();

    let (sql, vals) = query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let sqlx = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let ret = dbx.fetch_optional(sqlx).await?;

    Ok(ret)
}

/// Deletes a single entity from the database by its primary key ID and **requires**
/// that the entity is found and successfully deleted.
///
/// This function calls `delete_opt` internally and returns a specific `EntityNotFound`
/// error if no entity matching the ID is found.
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The type representing the deleted row (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `id`: The primary key identifier (`StoreId`) of the entity to delete.
/// * `meta`: Metadata including the target table and the primary key column name.
///
/// # Returns
///
/// A `StoreResult<T>` containing:
/// * `Ok(T)`: The fully deleted entity.
/// * `Err(StoreError::EntityNotFound)`: If no row was found matching the `id`.
/// * `Err(StoreError)`: If the underlying query execution fails.
pub async fn delete<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    id: &impl StoreId,
    meta: &MutateQueryMeta<I>,
) -> StoreResult<T> {
    match delete_opt(ctx, dbx, id, meta).await? {
        Some(t) => Ok(t),
        None => Err(StoreError::EntityNotFound {
            entity: meta.table.to_string(),
            id: id.to_string(),
        }),
    }
}

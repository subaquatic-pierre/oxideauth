use modql::field::HasSeaFields;
use modql::filter::{FilterGroups, ListOptions};
use sea_query::{
    Alias, Asterisk, Condition, Expr, IntoValueTuple, PostgresQueryBuilder, Query,
};
use sea_query::{Iden, IntoIden, TableRef, ExprTrait};
use crate::store::utils::pg_binder::PgBinder;
use sqlx::{postgres::PgRow, FromRow};
use sqlx::{query_as_with, Value};
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
        //
        // 🔑 Key Security Takeaway (The Guardrail):
        // If the context is scoped (e.g., ws_id = 'A'), the condition workspace_id = 'A' is added first.
        // If the user's filter also contained a workspace_id (e.g., trying to search for workspace_id = 'B'),
        // the final query becomes:
        // $$\text{WHERE} \ (\text{workspace\_id} = 'A') \ \mathbf{AND} \ (\text{workspace\_id} = 'B' \ \text{AND} \ \text{name} = 'Some') \ldots$$
        // Since $\text{workspace\_id} = 'A' \ \mathbf{AND} \ \text{workspace\_id} = 'B'$ (where $A \neq B$)
        // is a logically false condition, the query will return zero results, thus preventing
        // the standard user from escaping their assigned workspace.
        // The context clause acts as a secure, enforced prefix to the user-provided filter,
        // ensuring the security boundary is never breached. The clauses are **ANDed**, not overridden.

        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(Expr::val(ws_id));
        query.and_where(workspace_id_expr);
    }

    // SCENARIO 2 & 3: CONTEXT IS GLOBAL (Admin/Global Token)
    // If the context is global, we do not enforce a scope here. The query will
    // rely entirely on the user-provided filter in Step 2. If the user provided
    // a workspace_id in the filter DTO (Scenario 2), it will be used. If not
    // (Scenario 3), all data will be retrieved.

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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use serde_json::{from_value, json};
    use serial_test::serial;
    use sqlx::{query_as, Postgres};
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use crate::{
        dev::init::init_test,
        store::{
            entities::{
                account::{
                    AccountFilter, AccountForCreate, AccountForUpdate, AccountIden, AccountMeta,
                    AccountRow,
                },
                permission::{PermissionForCreate, PermissionRow},
                workspace::WorkspaceForCreate,
            },
            error::StoreError,
            queries::batch::create_many,
            stores::{account::AccountStore, permission::PermissionStore},
            traits::{
                crud::{Create, CreateMany, Delete, Get, List, Update},
                meta::{MutateStore, ReadStore},
            },
            utils::time_to_string,
        },
    };

    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_and_get_pass() -> StoreResult<()> {
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();

        let acc_store = AccountStore::new(dbx.clone());

        let ctx = StoreCtx::bootstrap();

        let mut data = AccountForCreate::default();
        data.email = "uninqueEmaeil@ema.c".to_string();

        let ret: AccountRow = create(&ctx, &dbx, data, &acc_store.mutate_meta()).await?;

        let found: AccountRow = acc_store.get(&ctx, &ret.id).await?;

        assert_eq!(found.id, ret.id);
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_get_fail() -> anyhow::Result<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        // Pick a random UUID that won't exist
        let missing_id = Uuid::new_v4();

        // Act
        let err = acc_store.get(&ctx, &missing_id.into()).await;

        matches!(err, Err(StoreError::EntityNotFound { .. }));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_filter_and_limit_pass() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        let meta = acc_store.mutate_meta();

        // Create accounts in two groups so filter can select only one group
        for i in 0..5 {
            let mut d = AccountForCreate::default();
            d.email = format!("in_{}@example.com", i);
            d.name = "LIST_FILTER_MATCH".to_string();
            create::<_, AccountRow, _, _>(&ctx, &dbx, d, &meta).await?;
        }
        for i in 0..2 {
            let mut d = AccountForCreate::default();
            d.email = format!("out_{}@example.com", i);
            d.name = "OTHER_PROVIDER".to_string();
            create::<_, AccountRow, _, _>(&ctx, &dbx, d, &meta).await?;
        }

        // Filter: name contains LIST_FILTER
        let filter: AccountFilter =
            from_value(json!({"name":{"$contains":"LIST_FILTER"}})).unwrap();

        // Limit to 3 results
        let opts = Some(ListOptions {
            limit: Some(3),
            offset: None,
            // add other fields if your ListOptions has them (e.g., sort, order, before/after)
            ..Default::default()
        });

        let meta = acc_store.read_meta();
        // Act
        let rows: Vec<AccountRow> = list(&ctx, &dbx, Some(filter), opts, &meta).await?;

        // Assert
        assert_eq!(rows.len(), 3, "list should respect the limit");
        // sanity: all rows must match the name filter
        assert!(rows.iter().all(|r| r.name.contains("LIST_FILTER")));
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_pagination_pass() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        let meta = acc_store.mutate_meta();

        // Create 6 rows in the matching group to test two pages of size 3
        for i in 0..6 {
            let mut d = AccountForCreate::default();
            d.email = format!("page_{}@example.com", i);
            d.name = "LIST_PAGINATION".to_string();
            create::<_, AccountRow, _, _>(&ctx, &dbx, d, &meta).await?;
        }

        let filter: AccountFilter = from_value(json!({"name":{"$eq":"LIST_PAGINATION"}})).unwrap();

        // Page 1: limit 3, offset 0
        let page1_opts = Some(ListOptions {
            limit: Some(3),
            offset: Some(0),
            ..Default::default()
        });
        let meta = acc_store.read_meta();
        let p1: Vec<AccountRow> = list(&ctx, &dbx, Some(filter), page1_opts, &meta).await?;
        assert_eq!(p1.len(), 3);

        // Page 2: limit 3, offset 3
        let page2_opts = Some(ListOptions {
            limit: Some(3),
            offset: Some(3),
            ..Default::default()
        });

        let filter: AccountFilter = from_value(json!({"name":{"$eq":"LIST_PAGINATION"}})).unwrap();
        let p2: Vec<AccountRow> = list(&ctx, &dbx, Some(filter), page2_opts, &meta).await?;
        assert_eq!(p2.len(), 3);

        // Ensure pages are disjoint by id
        let p1_ids: std::collections::HashSet<_> = p1.iter().map(|r| r.id).collect();
        let p2_ids: std::collections::HashSet<_> = p2.iter().map(|r| r.id).collect();
        assert!(p1_ids.is_disjoint(&p2_ids), "pages should be disjoint");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_invalid_limit_fail() -> anyhow::Result<()> {
        use serde_json::{from_value, json};

        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        let mutate_meta = acc_store.mutate_meta();
        let mut d = AccountForCreate::default();
        d.email = "limit_fail@example.com".into();
        d.name = "LIMIT_FAIL".into();
        create::<_, AccountRow, _, _>(&ctx, &dbx, d, &mutate_meta).await?;

        let filter: Option<AccountFilter> =
            Some(from_value(json!({"name":{"$eq":"LIMIT_FAIL"}})).unwrap());

        let opts = Some(ListOptions {
            limit: Some(0),
            offset: None,
            ..Default::default()
        });

        // Act
        let read_meta = acc_store.read_meta();
        let err = list::<_, AccountRow, _, _>(&ctx, &dbx, filter, opts, &read_meta).await;

        // Assert
        matches!(err, Err(StoreError::ListLimitExceeded { .. }));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_success() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        // Create a row to update
        let mut d = AccountForCreate::default();
        d.email = "update_ok@example.com".into();
        d.name = "UPDATE_BEFORE".into();
        let mutate_meta = acc_store.mutate_meta();
        let created: AccountRow = create(&ctx, &dbx, d, &mutate_meta).await?;

        // Prepare update (change name)
        let mut u = AccountForUpdate::default();
        u.name = Some("UPDATE_AFTER".into());

        // Act
        let update_meta = acc_store.mutate_meta();
        let updated: AccountRow = update(&ctx, &dbx, &created.id, u, &update_meta).await?;

        // Assert
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.name, "UPDATE_AFTER");
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_fail_not_found() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        // Non-existent ID
        let missing_id = Uuid::new_v4();

        let mut u = AccountForUpdate::default();
        u.name = Some("WON'T_APPLY".into());

        // Act
        let meta = acc_store.mutate_meta();
        let err = update::<_, AccountRow, _, _>(&ctx, &dbx, &missing_id, u, &meta).await;

        // Assert
        matches!(err, Err(StoreError::EntityNotFound { .. }));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_success() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        // Create a row to delete
        let mut d = AccountForCreate::default();
        d.email = "delete_ok@example.com".into();
        d.name = "DELETE_ME".into();
        let mutate_meta = acc_store.mutate_meta();
        let created: AccountRow = create(&ctx, &dbx, d, &mutate_meta).await?;

        // Act
        let delete_meta = acc_store.mutate_meta();
        let deleted: AccountRow = delete(&ctx, &dbx, &created.id, &delete_meta).await?;

        // Assert
        assert_eq!(deleted.id, created.id);
        assert_eq!(deleted.name, "DELETE_ME");
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_fail_not_found() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        // Non-existent ID
        let missing_id = Uuid::new_v4();

        // Act
        let meta = acc_store.mutate_meta();
        let err = delete::<_, AccountRow, _>(&ctx, &dbx, &missing_id, &meta).await;

        // Assert
        matches!(err, Err(StoreError::EntityNotFound { .. }));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_tags() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // Create a baseline account
        let mut create = AccountForCreate::default();
        create.email = "tag-update@example.com".to_string();
        create.name = "TEST_UPDATE_TAGS".to_string();
        let created: AccountRow = store.create(&ctx, create).await?;

        // Act: update tags
        let new_tags = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
        let mut upd = AccountForUpdate::default();
        upd.tags = Some(new_tags.clone());

        let updated: AccountRow = store.update(&ctx, &created.id, upd).await?;

        // Assert (via returned row)
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.tags, new_tags.as_slice());

        // Assert (via get)
        let fetched = store.get(&ctx, &created.id).await?;
        assert_eq!(fetched.tags, new_tags.as_slice());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_meta() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = AccountStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // Create a baseline account
        let mut create = AccountForCreate::default();
        create.email = "meta-update@example.com".to_string();
        create.name = "TEST_UPDATE_META".to_string();
        let created: AccountRow = store.create(&ctx, create).await?;

        // Act: update meta
        let mut upd = AccountForUpdate::default();
        upd.meta = Some(AccountMeta {
            schema_version: "v2".to_string(),
        });

        let updated: AccountRow = store.update(&ctx, &created.id, upd).await?;

        // Assert (via returned row)
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.meta.schema_version, "v2");

        // Assert (via get)
        let fetched = store.get(&ctx, &created.id).await?;
        assert_eq!(fetched.meta.schema_version, "v2");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_filter_by_created_by() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        let name_tag = "TEST_LIST_FILTER_BY_CREATED_BY";
        let mut data = vec![];
        for i in 0..3 {
            let mut ac = AccountForCreate::default();
            ac.email = format!("lfcb-{i}-{i}@example.com");
            ac.name = name_tag.into();
            data.push(ac)
        }
        let meta = store.mutate_meta();

        create_many::<_, AccountRow, AccountForCreate, AccountIden>(&ctx, &dbx, data, &meta)
            .await?;

        let filter = AccountFilter::try_from(serde_json::json!({
            "name": name_tag,
            "created_by":  ctx.user_id
        }))?;

        // Act
        let meta = store.read_meta();
        let rows: Vec<AccountRow> = list(&ctx, &dbx, Some(filter), None, &meta).await?;

        // Assert
        assert!(!rows.is_empty());
        for r in &rows {
            assert_eq!(r.name, name_tag);
            assert_eq!(r.audit.created_by, ctx.user_id.into());
        }

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_filter_by_created_at() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        let name_tag = "TEST_LIST_FILTER_BY_CREATED_AT";

        let start = OffsetDateTime::now_utc() - Duration::minutes(1);
        let mut data = vec![];
        for i in 0..3 {
            let mut ac = AccountForCreate::default();
            ac.email = format!("lfcb-{}@example.com", i);
            ac.name = name_tag.into();
            data.push(ac)
        }
        let meta = store.mutate_meta();
        create_many::<_, AccountRow, AccountForCreate, AccountIden>(&ctx, &dbx, data, &meta)
            .await?;

        let end = OffsetDateTime::now_utc() + Duration::minutes(1);

        let filter: AccountFilter = serde_json::json!({
            "name": { "$eq": name_tag },
            "created_at": {
                "$gte": time_to_string(start),
                "$lte": time_to_string(end)
            }
        })
        .try_into()?;

        // Act
        let meta = store.read_meta();
        let rows: Vec<AccountRow> = list(&ctx, &dbx, Some(filter), None, &meta).await?;

        // Assert
        assert!(!rows.is_empty());
        // for r in &rows {
        //     assert_eq!(r.name, name_tag);
        //     assert!(r.audit.created_at >= start && r.audit.created_at <= end);
        // }

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_order_by_created_at() -> StoreResult<()> {
        use tokio::time::{sleep, Duration};

        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap();

        let name_tag = "TEST_LIST_ORDER_BY_CREATED_AT";

        let mutate_meta = store.mutate_meta();

        let mut a1 = AccountForCreate::default();
        a1.email = "loca-1@example.com".into();
        a1.name = name_tag.into();
        let r1: AccountRow = create(&ctx, &dbx, a1, &mutate_meta).await?;

        sleep(Duration::from_millis(10)).await;

        let mut a2 = AccountForCreate::default();
        a2.email = "loca-2@example.com".into();
        a2.name = name_tag.into();
        let r2: AccountRow = create(&ctx, &dbx, a2, &mutate_meta).await?;

        let filter = AccountFilter::try_from(serde_json::json!({
            "name": { "$eq": name_tag }
        }))?;

        let mut opts = ListOptions::default();
        opts.order_bys = Some(vec!["created_at".to_string()].into());
        opts.limit = Some(10);

        // Act
        let read_meta = store.read_meta();
        let rows: Vec<AccountRow> = list(&ctx, &dbx, Some(filter), Some(opts), &read_meta).await?;

        // Assert
        let emails: Vec<_> = rows.iter().map(|x| x.email.as_str()).collect();
        let idx1 = emails
            .iter()
            .position(|e| *e == "loca-1@example.com")
            .unwrap();
        let idx2 = emails
            .iter()
            .position(|e| *e == "loca-2@example.com")
            .unwrap();
        assert!(idx1 < idx2, "expected loca-1 before loca-2 with ctime ASC");
        assert!(r1.audit.created_at <= r2.audit.created_at);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_enforces_workspace_scope() -> StoreResult<()> {
        // Arrange
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();

        let store = PermissionStore::new(dbx.clone());
        let mutate_meta = store.mutate_meta();

        // 1. Define the official, enforced workspace ID from the context (WS_A)
        // Use a real workspace so the created permission satisfies the FK.
        let ctx = StoreCtx::bootstrap();
        let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
        let enforced_ws_id: Uuid = ws.id.into();
        let user_id = Uuid::new_v4();
        let mut scoped_ctx = StoreCtx::new(user_id, enforced_ws_id);
        scoped_ctx.set_workspace_scope(Some(enforced_ws_id));

        // 2. Define the forged workspace ID in the DTO (WS_B)
        let forged_ws_id = Uuid::new_v4();
        assert_ne!(enforced_ws_id, forged_ws_id, "Test IDs must be distinct");

        // 3. Prepare DTO with the forged ID
        let mut data = PermissionForCreate::default();
        data.workspace_id = forged_ws_id;
        data.name = "Scoped_Permission_Test".to_string();

        // Act
        // The create function should now call prepare_workspace_scope and overwrite
        // data.workspace_id with scoped_ctx.workspace_scope().
        let created_row: PermissionRow = create(&scoped_ctx, &dbx, data, &mutate_meta).await?;

        // Assert

        // 1. Verify the returned row contains the enforced ID
        assert_eq!(
            created_row.workspace_id, enforced_ws_id,
            "Created row must have the context's enforced workspace_id."
        );

        // 2. Verify by fetching from the database (most rigorous check)
        let fetched_row: PermissionRow = store.get(&scoped_ctx, &created_row.id).await?;

        assert_eq!(
            fetched_row.workspace_id, enforced_ws_id,
            "Fetched row from DB must confirm the context's enforced workspace_id."
        );

        // Ensure the forged ID was NOT used
        assert_ne!(
            fetched_row.workspace_id, forged_ws_id,
            "The forged DTO workspace_id must have been overridden."
        );

        Ok(())
    }
}

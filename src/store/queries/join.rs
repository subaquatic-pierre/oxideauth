use modql::filter::{FilterGroups, ListOptions};
use sea_query::{
    Asterisk, CommonTableExpression, Condition, Expr, Func, Iden, JoinType, OnConflict,
    PostgresQueryBuilder, Query, SelectStatement, SimpleExpr, Value, ExprTrait,
};
use crate::store::utils::pg_binder::PgBinder;
use serde_json::Value as JsonValue;
use serde_json::json;
use sqlx::{FromRow, postgres::PgRow};

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::workspace::WorkspaceIden,
    error::{StoreError, StoreResult},
    queries::{
        count::{count, count_many},
        meta::{CountManyQueryMeta, ManyToManyQueryMeta, OneToManyQueryMeta, ReadQueryMeta},
    },
    traits::{
        dbx::DbExecutor,
        meta::{HasId, StoreId, StoreRow, TableIden},
    },
    utils::{apply_tags_to_query, LIST_LIMIT_MAX, ListOptionsValidator, pg_type_of},
};

#[derive(Iden)]
pub struct ManyCte;

/// Retrieves a single "parent" entity and its associated "many" collection entities,
/// joined and aggregated into a single returned type (`T`).
///
/// This performs a complex query using a **Common Table Expression (CTE)** and a
/// `LEFT JOIN` with `jsonb_agg` to embed the related "many" rows as a JSON array
/// within the resulting "parent" entity.
///
/// # Logic
///
/// 1. **Limit Check**: Uses `count_many` to check if the number of "many" children
///    associated with the "parent" ID exceeds `LIST_LIMIT_MAX`.
/// 2. **CTE**: Defines the "many" table selection within a CTE to simplify the main query.
/// 3. **Aggregation**: Performs a `LEFT JOIN` from the "parent" table to the CTE and
///    uses `jsonb_agg` (aliased via `meta.agg_alias`) to collect all "many" rows into a JSON array.
/// 4. **Grouping**: Groups the results by the "parent" primary key (`meta.single_pk`).
///
/// # Type Parameters
///
/// * `E`: The database executor (`DbExecutor`).
/// * `T`: The custom result type, containing the parent row fields and the aggregated child collection (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `id`: The primary key ID of the single "parent" entity to retrieve.
/// * `meta`: Metadata defining the relationship: "parent" table/PK, "many" table/FK/PK, and the aggregation alias.
///
/// # Returns
///
/// A `StoreResult<Option<T>>` containing:
/// * `Ok(Some(T))`: The parent entity with its collection of children embedded.
/// * `Ok(None)`: If the parent entity was not found.
/// * `Err(StoreError::ListLimitExceeded)`: If the count of children exceeds `LIST_LIMIT_MAX`.
/// * `Err(StoreError)`: If the query execution fails.
pub async fn get_one_to_many_opt<E: DbExecutor, T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &E,
    id: &impl StoreId,
    meta: &OneToManyQueryMeta<I>,
) -> StoreResult<Option<T>> {
    // Guard: count rows on the many side via its FK -> single PK
    let count_meta = CountManyQueryMeta {
        table: meta.many_table,
        fk: meta.many_fk,
    };
    let count = count_many(ctx, dbx, id, &count_meta).await?;
    if count > LIST_LIMIT_MAX {
        return Err(StoreError::ListLimitExceeded {
            max: LIST_LIMIT_MAX,
            actual: count,
        });
    }

    // Define the query for the Common Table Expression (CTE)
    let cte_query = Query::select()
        .from(meta.many_table)
        .column(Asterisk)
        .to_owned();

    // Create the CTE object
    let common_table_expr = CommonTableExpression::new()
        .table_name(ManyCte) // Name our CTE `CredentialCte`
        .query(cte_query) // Use the query defined above
        .to_owned();

    // Create custom aggregate and coalesce expression
    let cust = format!(
        r#"COALESCE(jsonb_agg("{many_cte}") FILTER (WHERE "{many_cte}"."{many_pk}" IS NOT NULL), '[]'::jsonb)"#,
        many_cte = ManyCte.to_string(),
        many_pk = meta.many_pk.to_string()
    );
    let join_agg = Expr::cust(cust);

    // Build the main query
    let mut main_query = Query::select();
    main_query
        .from(meta.single_table)
        .column((meta.single_table, Asterisk))
        // we need to use meta.agg_alias in order to deserialize to the correct field on the type T which is `single_table_row.collection`, ie. account.credentials
        .expr_as(join_agg, meta.agg_alias)
        .join(
            JoinType::LeftJoin,
            ManyCte, // Join to the CTE, not the original table
            Expr::col((meta.single_table, meta.single_pk)).equals((ManyCte, meta.many_fk)),
        )
        .and_where(Expr::col((meta.single_table, meta.single_pk)).eq(Expr::val(id.clone())))
        .group_by_col((meta.single_table, meta.single_pk));

    // Attach the WithClause to the main query
    let final_query = main_query.with(common_table_expr.into());

    let (sql, vals) = final_query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let res: Option<T> = dbx.fetch_optional(query).await?;

    Ok(res)
}

/// Retrieves a single "parent" entity and its associated "many" collection,
/// requiring that the parent entity is found.
///
/// This function calls `get_one_to_many_opt` internally. If the parent entity is not
/// found, it returns a specific `EntityNotFound` error.
///
/// # Type Parameters
///
/// * `T`: The custom result type, containing the parent row fields and the aggregated child collection (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `id`: The primary key ID of the single "parent" entity to retrieve.
/// * `meta`: Metadata defining the one-to-many relationship tables and keys.
///
/// # Returns
///
/// A `StoreResult<T>` containing:
/// * `Ok(T)`: The parent entity with its collection of children embedded.
/// * `Err(StoreError::EntityNotFound)`: If the parent entity was not found.
/// * `Err(StoreError)`: If the underlying query execution or list limit check fails.
pub async fn get_one_to_many<T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &impl DbExecutor,
    id: &impl StoreId,
    meta: &OneToManyQueryMeta<I>,
) -> StoreResult<T> {
    match get_one_to_many_opt(ctx, dbx, id, meta).await? {
        Some(t) => Ok(t),
        None => Err(StoreError::EntityNotFound {
            entity: meta.single_table.to_string(),
            id: id.to_string(),
        }),
    }
}

/// Retrieves a list of "parent" entities, where each parent entity includes its
/// associated "many" collection entities aggregated as a JSON array.
///
/// This function performs a list operation on the **parent table** (`meta.single_table`),
/// applies filtering and list options to the parents, and embeds the related child data.
///
/// # Logic
///
/// 1. **Limit Check**: Counts the total number of **parent** rows matching the filter and checks if the total exceeds `LIST_LIMIT_MAX`.
/// 2. **CTE**: Defines the "many" table selection within a CTE.
/// 3. **Query**: Uses a `LEFT JOIN` and `jsonb_agg` to join the parent table to the child CTE, aggregating child rows into a JSON array (aliased via `meta.agg_alias`).
/// 4. **Filtering/Options**: Applies the `filter` and `opts` (limit, offset, order) to the **parent** query.
///
/// # Type Parameters
///
/// * `T`: The custom result type, containing the parent row fields and the aggregated child collection (`StoreRow`).
/// * `F`: A type convertible into `FilterGroups` and must be `Clone` for the initial count check.
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `filter`: An optional filter applied to the **parent table**.
/// * `opts`: Optional `ListOptions` for sorting and pagination of the **parent table**.
/// * `meta`: Metadata defining the relationship tables, keys, and aggregation alias.
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing:
/// * `Ok(Vec<T>)`: A list of parent entities, each with their associated child collection embedded.
/// * `Err(StoreError::ListLimitExceeded)`: If the total count of parent rows exceeds `LIST_LIMIT_MAX`.
/// * `Err(StoreError)`: If the query execution or option validation fails.
pub async fn list_one_to_many<T: StoreRow, F: Into<FilterGroups> + Clone, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &impl DbExecutor,
    filter: Option<F>,
    opts: Option<ListOptions>,
    meta: &OneToManyQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    let count_meta = ReadQueryMeta {
        table: meta.single_table,
        pk: meta.single_pk,
        has_audit: meta.has_audit,
    };
    let count = count(ctx, dbx, filter.clone(), &count_meta).await?;
    if count > LIST_LIMIT_MAX {
        return Err(StoreError::ListLimitExceeded {
            max: LIST_LIMIT_MAX,
            actual: count,
        });
    }

    // Define the query for the Common Table Expression (CTE)
    let cte_query = Query::select()
        .from(meta.many_table)
        .column(Asterisk)
        .to_owned();

    // Create the CTE object
    let common_table_expr = CommonTableExpression::new()
        .table_name(ManyCte) // Name our CTE `CredentialCte`
        .query(cte_query) // Use the query defined above
        .to_owned();

    let cust = format!(
        r#"COALESCE(jsonb_agg("{many_cte}") FILTER (WHERE "{many_cte}"."{many_pk}" IS NOT NULL), '[]'::jsonb)"#,
        many_cte = ManyCte.to_string(),
        many_pk = meta.many_pk.to_string()
    );
    let join_agg = Expr::cust(cust);

    // 4. Build the main query
    let mut main_query = Query::select();
    main_query
        .from(meta.single_table)
        .column((meta.single_table, Asterisk))
        .expr_as(join_agg, meta.agg_alias);

    // apply workspace scope
    if let Some(ws_id) = ctx.workspace_scope() {
        let workspace_id_expr =
            Expr::col((meta.single_table, WorkspaceIden::WorkspaceId)).eq(Expr::val(ws_id));
        main_query.and_where(workspace_id_expr);
    }

    // apply filter BEFORE join/group_by so it generates WHERE, not HAVING
    if let Some(filter) = filter {
        let filters: FilterGroups = filter.into();
        let cond: Condition = filters.try_into()?;
        main_query.cond_where(cond);
    }

    main_query
        .join(
            JoinType::LeftJoin,
            ManyCte, // Join to the CTE, not the original table
            Expr::col((meta.single_table, meta.single_pk)).equals((ManyCte, meta.many_fk)),
        )
        .group_by_col((meta.single_table, meta.single_pk));

    // validate list options
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;
    // add list options to query, there will always at least be maximum limit
    list_options.apply_to_sea_query(&mut main_query);

    // Attach the WithClause to the main query
    let final_query = main_query.with(common_table_expr.into());

    let (sql, vals) = final_query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let res: Vec<T> = dbx.fetch_all(query).await?;

    Ok(res)
}

/// Retrieves a single "parent" entity and its associated "many" collection entities
/// through an intermediary **join table**, embedding the related "many" rows as a JSON array.
///
/// This performs a complex query using a **Common Table Expression (CTE)** and multiple
/// `LEFT JOIN`s to link the parent, the join table, and the final collection table.
///
/// # Type Parameters
///
/// * `T`: The custom result type, containing the parent row fields and the aggregated child collection (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `id`: The primary key ID of the single "parent" entity to retrieve.
/// * `meta`: Metadata defining the many-to-many relationship: parent table/PK, join table/FKs, many table/PK, and aggregation alias.
///
/// # Returns
///
/// A `StoreResult<Option<T>>` containing:
/// * `Ok(Some(T))`: The parent entity with its collection of children embedded.
/// * `Ok(None)`: If the parent entity was not found.
/// * `Err(StoreError::ListLimitExceeded)`: If the count of join-table entries exceeds `LIST_LIMIT_MAX`.
/// * `Err(StoreError)`: If the query execution fails.
pub async fn get_many_to_many_opt<T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &impl DbExecutor,
    id: &impl StoreId,
    meta: &ManyToManyQueryMeta<I>,
) -> StoreResult<Option<T>> {
    // Guard: count rows on the many side via its FK -> single PK
    let count_meta = CountManyQueryMeta {
        table: meta.join_table,
        fk: meta.join_fk,
    };
    let count = count_many(ctx, dbx, id, &count_meta).await?;
    if count > LIST_LIMIT_MAX {
        return Err(StoreError::ListLimitExceeded {
            max: LIST_LIMIT_MAX,
            actual: count,
        });
    }

    // Define the query for the Common Table Expression (CTE)
    let cte_query = Query::select()
        .from(meta.many_table)
        .column(Asterisk)
        .to_owned();

    // Create the CTE object
    let common_table_expr = CommonTableExpression::new()
        .table_name(ManyCte) // Name our CTE `CredentialCte`
        .query(cte_query) // Use the query defined above
        .to_owned();

    // Create custom aggregate and coalesce expression
    let cust = format!(
        r#"COALESCE(jsonb_agg("{many_cte}") FILTER (WHERE "{many_cte}"."{many_pk}" IS NOT NULL), '[]'::jsonb)"#,
        many_cte = ManyCte.to_string(),
        many_pk = meta.many_pk.to_string()
    );
    let join_agg = Expr::cust(cust);

    // Build the main query
    let mut main_query = Query::select();
    main_query
        .from(meta.single_table)
        .column((meta.single_table, Asterisk))
        // we need to use meta.agg_alias in order to deserialize to the correct field on the type T which is `single_table_row.collection`, ie. account.credentials
        .expr_as(join_agg, meta.agg_alias)
        .join(
            JoinType::LeftJoin,
            meta.join_table,
            // meta.join_fk: correlates to single table "primary key", ie. role_permission.role_id
            Expr::col((meta.join_table, meta.join_fk)).equals((meta.single_table, meta.single_pk)),
        )
        .join(
            JoinType::LeftJoin,
            ManyCte, // Join to the CTE, not the original table to avoid column collisions
            // meta.many_fk: correlates to CTE "primary_key", ie. role_permission.permission_id
            Expr::col((meta.join_table, meta.many_fk)).equals((ManyCte, meta.many_pk)),
        )
        .and_where(Expr::col((meta.single_table, meta.single_pk)).eq(Expr::val(id.clone())))
        .group_by_col((meta.single_table, meta.single_pk));

    // Attach the WithClause to the main query
    let final_query = main_query.with(common_table_expr.into());

    let (sql, vals) = final_query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let res: Option<T> = dbx.fetch_optional(query).await?;

    Ok(res)
}

/// Retrieves a single **"parent"** entity and its associated **"many"** collection
/// (via a join table), and **requires** that the parent entity is found.
///
/// This function delegates to `get_many_to_many_opt` and converts a `None` result
/// into a `StoreError::EntityNotFound`.
///
/// # Type Parameters
///
/// * `T`: The custom result type, containing the parent row and the embedded child collection (`StoreRow`).
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `id`: The primary key ID of the single "parent" entity to retrieve.
/// * `meta`: Metadata defining the many-to-many relationship tables and keys.
///
/// # Returns
///
/// A `StoreResult<T>` containing:
/// * `Ok(T)`: The parent entity with its collection of children embedded.
/// * `Err(StoreError::EntityNotFound)`: If the parent entity was not found.
/// * `Err(StoreError)`: If the underlying query execution or list limit check fails.
pub async fn get_many_to_many<T: StoreRow, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &impl DbExecutor,
    id: &impl StoreId,
    meta: &ManyToManyQueryMeta<I>,
) -> StoreResult<T> {
    match get_many_to_many_opt(ctx, dbx, id, meta).await? {
        Some(t) => Ok(t),
        None => Err(StoreError::EntityNotFound {
            entity: meta.single_table.to_string(),
            id: id.to_string(),
        }),
    }
}

/// Retrieves a list of **"parent"** entities, where each parent entity includes its
/// associated **"many"** collection entities aggregated via an intermediary **join table**.
///
/// This function performs a list operation on the **parent table** (`meta.single_table`),
/// applies filtering and list options to the parents, and embeds the related child data
/// using a CTE and `jsonb_agg`.
///
/// # Type Parameters
///
/// * `T`: The custom result type, containing the parent row fields and the aggregated child collection (`StoreRow`).
/// * `F`: A type convertible into `FilterGroups` and must be `Clone` for the initial count check.
/// * `I`: The table identifier (`TableIden`).
///
/// # Arguments
///
/// * `ctx`: The store context.
/// * `dbx`: The database executor.
/// * `filter`: An optional filter applied to the **parent table**.
/// * `opts`: Optional `ListOptions` for sorting and pagination of the **parent table**.
/// * `meta`: Metadata defining the many-to-many relationship tables, keys, and aggregation alias.
///
/// # Returns
///
/// A `StoreResult<Vec<T>>` containing:
/// * `Ok(Vec<T>)`: A list of parent entities, each with their associated child collection embedded.
/// * `Err(StoreError::ListLimitExceeded)`: If the total count of parent rows exceeds `LIST_LIMIT_MAX`.
/// * `Err(StoreError)`: If the query execution or option validation fails.
pub async fn list_many_to_many<T: StoreRow, F: Into<FilterGroups> + Clone, I: TableIden>(
    ctx: &StoreCtx,
    dbx: &impl DbExecutor,
    tags: Option<Vec<String>>,
    filter: Option<F>,
    opts: Option<ListOptions>,
    meta: &ManyToManyQueryMeta<I>,
) -> StoreResult<Vec<T>> {
    let count_meta = ReadQueryMeta {
        table: meta.single_table,
        pk: meta.join_fk,
        has_audit: false,
    };
    let count = count(ctx, dbx, filter.clone(), &count_meta).await?;
    if count > LIST_LIMIT_MAX {
        return Err(StoreError::ListLimitExceeded {
            max: LIST_LIMIT_MAX,
            actual: count,
        });
    }

    // Define the query for the Common Table Expression (CTE)
    let cte_query = Query::select()
        .from(meta.many_table)
        .column(Asterisk)
        .to_owned();

    // Create the CTE object
    let common_table_expr = CommonTableExpression::new()
        .table_name(ManyCte) // Name our CTE `CredentialCte`
        .query(cte_query) // Use the query defined above
        .to_owned();

    // Create custom aggregate and coalesce expression
    let cust = format!(
        r#"COALESCE(jsonb_agg("{many_cte}") FILTER (WHERE "{many_cte}"."{many_pk}" IS NOT NULL), '[]'::jsonb)"#,
        many_cte = ManyCte.to_string(),
        many_pk = meta.many_pk.to_string()
    );
    let join_agg = Expr::cust(cust);

    // Build the main query
    let mut main_query = Query::select();
    main_query
        .from(meta.single_table)
        .column((meta.single_table, Asterisk))
        // we need to use meta.agg_alias in order to deserialize to the correct field on the type T which is `single_table_row.collection`, ie. account.credentials
        .expr_as(join_agg, meta.agg_alias)
        .join(
            JoinType::LeftJoin,
            meta.join_table,
            // meta.join_fk: correlates to single table "primary key", ie. role_permission.role_id
            Expr::col((meta.join_table, meta.join_fk)).equals((meta.single_table, meta.single_pk)),
        )
        .join(
            JoinType::LeftJoin,
            ManyCte, // Join to the CTE, not the original table to avoid column collisions
            // meta.many_fk: correlates to CTE "primary_key", ie. role_permission.permission_id
            Expr::col((meta.join_table, meta.many_fk)).equals((ManyCte, meta.many_pk)),
        )
        .group_by_col((meta.single_table, meta.single_pk));

    if let Some(filter) = filter {
        let filters: FilterGroups = filter.into();
        let cond: Condition = filters.try_into()?;
        main_query.cond_where(cond);
    }

    // --- Tags containment (@>) ---
    if let Some(tags) = tags {
        apply_tags_to_query(&mut main_query, meta.single_table, tags);
    }

    // --- Workspace scoping ---
    if let Some(ws_id) = ctx.workspace_scope() {
        let workspace_id_expr =
            Expr::col((meta.single_table, WorkspaceIden::WorkspaceId)).eq(Expr::val(ws_id));
        main_query.and_where(workspace_id_expr);
    }

    // validate list options
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;
    // add list options to query, there will always at least be maximum limit
    list_options.apply_to_sea_query(&mut main_query);

    // Attach the WithClause to the main query
    let final_query = main_query.with(common_table_expr.into());

    let (sql, vals) = final_query.build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let res: Vec<T> = dbx.fetch_all(query).await?;

    Ok(res)
}

/// Replaces the entire set of "many-side" associations for a single "self-side"
/// entity in a many-to-many relationship.
///
/// This operation ensures that after execution, the join table only contains links
/// specified by the `other_ids` for the given `self_id`.
///
/// # Logic
///
/// 1. **Delete**: All existing rows in the join table where the `meta.join_fk` matches `self_id` are deleted.
/// 2. **Insert**: If `other_ids` is not empty, new rows are inserted into the join table for every `other_id`, linking them to `self_id`.
///
/// # Type Parameters
///
/// * `I`: The table identifier (`TableIden`) for metadata.
/// * `ID`: The type of the primary key used for both `self_id` and `other_ids` (must be `StoreId` and `Clone`).
///
/// # Arguments
///
/// * `ctx`: The store context (reserved for future audit logging).
/// * `dbx`: The database executor.
/// * `self_id`: The ID of the "parent" entity whose associations are being managed. This is the value for `meta.join_fk`.
/// * `other_ids`: A vector of IDs for the "many" entities that should be linked to `self_id`. These are the values for `meta.many_fk`.
/// * `meta`: Metadata defining the many-to-many relationship, specifically the `join_table`, `join_fk`, and `many_fk`.
///
/// # Returns
///
/// A `StoreResult<()>`:
/// * `Ok(())`: If all delete and insert operations complete successfully.
/// * `Err(StoreError)`: If any underlying database execution fails.
pub async fn set_many_to_many_links<I: TableIden, ID: StoreId + Clone>(
    ctx: &StoreCtx, // Ctx might be used for auditing in the future
    dbx: &impl DbExecutor,
    self_id: &ID,
    other_ids: Vec<ID>,
    meta: &ManyToManyQueryMeta<I>,
) -> StoreResult<()> {
    // // Delete all existing associations for self_id
    let (sql, vals) = Query::delete()
        .from_table(meta.join_table)
        .and_where(Expr::col(meta.join_fk).eq(Expr::val(self_id.clone())))
        .build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_with(&sql, vals);

    let _ = dbx.execute(query).await?;

    // If there are new IDs to link, insert them
    if !other_ids.is_empty() {
        let mut query = Query::insert();
        query
            .into_table(meta.join_table)
            .columns([meta.join_fk, meta.many_fk]);

        // Add a row for each new association
        for other_id in other_ids {
            // Convert each ID directly into a SimpleExpr using .into()
            let row = vec![
                Expr::value(self_id.clone().into()),
                Expr::value(other_id.into()),
            ];
            query.values_panic(row);
        }

        let (sql, vals) = query.build(PostgresQueryBuilder);
        let vals = PgBinder(vals.0);
        let query = sqlx::query_with(&sql, vals);
        let _ = dbx.execute(query).await?;
    }

    Ok(())
}

/// Creates a single association between two entities in a many-to-many relationship.
///
/// This function inserts a single row into the join table linking `self_id` and `other_id`.
/// It is **idempotent**, meaning if the link already exists, the `ON CONFLICT DO NOTHING`
/// clause prevents an error.
///
/// # Type Parameters
///
/// * `I`: The table identifier (`TableIden`) for metadata.
/// * `ID`: The ID type used for the linked entities (must be `StoreId`).
///
/// # Arguments
///
/// * `_ctx`: The store context (currently ignored, but kept for signature consistency).
/// * `dbx`: The database executor.
/// * `self_id`: The ID of the "parent" entity (`meta.join_fk`).
/// * `other_id`: The ID of the entity being attached (`meta.many_fk`).
/// * `meta`: Metadata defining the many-to-many join table and foreign key column names.
///
/// # Returns
///
/// A `StoreResult<()>`:
/// * `Ok(())`: If the link was successfully created or already existed (idempotent success).
/// * `Err(StoreError)`: If the underlying database execution fails.
pub async fn attach_link<I: TableIden, ID: StoreId>(
    _ctx: &StoreCtx,
    dbx: &impl DbExecutor,
    self_id: &ID,
    other_id: &ID,
    meta: &ManyToManyQueryMeta<I>,
) -> StoreResult<()> {
    // Build an INSERT statement for a single row.
    let (sql, vals) = Query::insert()
        .into_table(meta.join_table)
        .columns([meta.join_fk, meta.many_fk])
        .values_panic(vec![
            Expr::value(self_id.clone().into()),
            Expr::value(other_id.clone().into()),
        ])
        // Optional: Add an "ON CONFLICT DO NOTHING" to prevent errors
        // if the link already exists. This makes the operation idempotent.
        .on_conflict(
            OnConflict::columns([meta.join_fk, meta.many_fk])
                .do_nothing()
                .to_owned(),
        )
        .build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_with(&sql, vals);

    let _ = dbx.execute(query).await?;

    Ok(())
}

/// Removes a specific association between two entities in a many-to-many relationship.
///
/// This function executes a `DELETE` query on the join table, targeting the row
/// identified by the combination of `self_id` and `other_id`. The operation is
/// **idempotent**; if the link doesn't exist, no error is returned.
///
/// # Type Parameters
///
/// * `I`: The table identifier (`TableIden`) for metadata.
/// * `ID`: The ID type used for the linked entities (must be `StoreId`).
///
/// # Arguments
///
/// * `_ctx`: The store context (currently ignored).
/// * `dbx`: The database executor.
/// * `self_id`: The ID of the "parent" entity to detach from (`meta.join_fk`).
/// * `other_id`: The ID of the "many" entity being detached (`meta.many_fk`).
/// * `meta`: Metadata defining the many-to-many join table and foreign key column names.
///
/// # Returns
///
/// A `StoreResult<()>`:
/// * `Ok(())`: If the link was successfully deleted or did not exist.
/// * `Err(StoreError)`: If the underlying database execution fails.
pub async fn detach_link<I: TableIden, ID: StoreId>(
    _ctx: &StoreCtx,
    dbx: &impl DbExecutor,
    self_id: &ID,
    other_id: &ID,
    meta: &ManyToManyQueryMeta<I>,
) -> StoreResult<()> {
    // Build a DELETE statement targeting the specific link.
    let (sql, vals) = Query::delete()
        .from_table(meta.join_table)
        // WHERE self_fk = 'self_id'
        .and_where(Expr::col(meta.join_fk).eq(Expr::val(self_id.clone())))
        // AND other_fk = 'other_id'
        .and_where(Expr::col(meta.many_fk).eq(Expr::val(other_id.clone())))
        .build(PostgresQueryBuilder);
    let vals = PgBinder(vals.0);

    let query = sqlx::query_with(&sql, vals);

    let _ = dbx.execute(query).await?;

    Ok(())
}

use modql::filter::{FilterGroups, ListOptions};
use sea_query::{Alias, Asterisk, BinOper, Condition, Expr, Func, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
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
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(ws_id);
        query.and_where(workspace_id_expr);
    }

    // validate list options
    // Assuming ListOptionsValidator and has_audit exist in scope/meta for the filter
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;

    // add list options to query (e.g., limit, offset, order_by)
    list_options.apply_to_sea_query(&mut query);

    let (sql, vals) = query.build_sqlx(PostgresQueryBuilder);

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
    if let Some(tags) = tags.filter(|t| !t.is_empty()) {
        let tags_values: Vec<JsonValue> = tags.into_iter().map(JsonValue::String).collect();
        let expr =
            Expr::cust_with_values(format!(r#""{}" @> $"#, meta.col.to_string()), [tags_values]);
        query.and_where(expr);
    }

    // --- Field-based filter ---
    if let Some(filter) = filter {
        let cond: Condition = filter.try_into()?;
        query.cond_where(cond);
    }

    // --- Workspace scoping ---
    if let Some(ws_id) = ctx.workspace_scope() {
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(ws_id);
        query.and_where(workspace_id_expr);
    }

    // --- List options (limit/offset/order) ---
    let list_options = ListOptionsValidator::validate_list_opts(opts, meta.has_audit)?;
    list_options.apply_to_sea_query(&mut query);

    let (sql, vals) = query.build_sqlx(PostgresQueryBuilder);

    debug!("SQL: {:#?}, VALS: {:#?}", sql, vals);

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
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(ws_id);
        query.and_where(workspace_id_expr);
    }

    query
        .expr_as(Func::count(Expr::col(Asterisk)), "count")
        .from(meta.table);

    // --- Tags containment (@>) ---
    if let Some(tags) = tags.filter(|t| !t.is_empty()) {
        let expr = Expr::col((meta.table, Alias::new("tags"))).binary(
            BinOper::Custom("@>"),
            Expr::val(tags).cast_as(Alias::new("text[]")),
        );
        query.and_where(expr);
    }

    // --- Field-based filter ---
    if let Some(filter) = filter {
        let cond: Condition = filter.try_into()?;
        query.cond_where(cond);
    }

    let (sql, vals) = query.build_sqlx(PostgresQueryBuilder);
    let sqlx_query = sqlx::query_as_with(&sql, vals);

    #[derive(sqlx::FromRow)]
    struct CountRow {
        count: i64,
    }

    let row: CountRow = dbx.fetch_one(sqlx_query).await?;
    Ok(row.count)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use env_logger::filter;
    use modql::filter::{OrderBy, OrderBys};
    use sea_query::Order;
    use serde_json::{from_value, json};
    use serial_test::serial;

    use uuid::Uuid; // Need Uuid for creating a separate workspace ID

    use crate::{
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{
                permission::{
                    PermissionFilter, PermissionForCreate, PermissionIden, PermissionMeta,
                    PermissionRow,
                },
                workspace::WorkspaceForCreate,
            },
            queries::crud::create,
            stores::permission::PermissionStore,
            traits::{
                crud::{Create, Get, List},
                meta::ReadStore,
            },
        },
    };

    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_meta() -> StoreResult<()> {
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx.clone());

        // Isolate this test in a dedicated fixture workspace (registrar) and
        // scope queries to it, so the exact-count assertions are unaffected by
        // other rows in the shared test DB (e.g. canonical permissions that are
        // seeded into every workspace created via the workspace service).
        let mut ctx = StoreCtx::bootstrap();
        let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
        let ws_id: Uuid = ws.id.into();
        ctx.set_workspace_scope(Some(ws_id));

        let c_perm = |i| {
            let mut perm = PermissionForCreate::default();
            perm.workspace_id = ws_id;
            perm.name = format!("PERMISSION_GET_MANY_TEST_{i}_i_{i}_i_{i}");
            if (i < 2) {
                perm.meta = PermissionMeta {
                    schema_version: "1".to_string(),
                };
            } else {
                perm.meta = PermissionMeta {
                    schema_version: "2".to_string(),
                };
            }
            perm
        };

        let perm_count = 5;
        let mut perms = vec![];

        for i in 0..perm_count {
            let n = c_perm(i);

            perms.push(store.create(&ctx, n).await?);
        }

        let meta = ContainsFilterQueryMeta {
            table: PermissionIden::Table,
            col: PermissionIden::Meta,
            has_audit: true,
        };

        let schema_1: Vec<PermissionRow> = filter_by_value_contains(
            &ctx,
            &dbx,
            ContainsFilter::Json(json!({"schema_version":"1"})),
            None,
            &meta,
        )
        .await?;

        let schema_2: Vec<PermissionRow> = filter_by_value_contains(
            &ctx,
            &dbx,
            ContainsFilter::Json(json!({"schema_version":"2"})),
            None,
            &meta,
        )
        .await?;

        assert_eq!(2, schema_1.len());
        assert_eq!(3, schema_2.len());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_tags() -> StoreResult<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx.clone());

        // Isolate this test in a dedicated fixture workspace (acme) and scope
        // queries to it, so the exact-count assertions are unaffected by other
        // rows in the shared test DB (e.g. canonical permissions that are seeded
        // into every workspace created via the workspace service).
        let mut ctx = StoreCtx::bootstrap();
        let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
        let ws_id: Uuid = ws.id.into();
        ctx.set_workspace_scope(Some(ws_id));

        // -- Create test data with different tags
        let c_perm = |i| {
            let mut perm = PermissionForCreate::default();
            perm.workspace_id = ws_id;
            perm.name = format!("PERMISSION_TAGS_TEST_{i}");
            // Assign different tags to two distinct groups
            if i < 2 {
                perm.tags = vec!["system".to_string(), "critical".to_string()];
            } else {
                perm.tags = vec!["user".to_string(), "general".to_string()];
            }
            perm
        };

        let perm_count = 5;
        for i in 0..perm_count {
            let n = c_perm(i);
            store.create(&ctx, n).await?;
        }

        // -- Define query metadata for the 'tags' column
        let meta = ContainsFilterQueryMeta {
            table: PermissionIden::Table,
            col: PermissionIden::Tags, // Assumes 'Tags' is a variant on your Iden
            has_audit: true,
        };

        // -- Run Filters and Assertions
        // Test 1: Filter by a single tag present in the first group of 2 permissions
        let system_perms: Vec<PermissionRow> = filter_by_value_contains(
            &ctx,
            &dbx,
            ContainsFilter::Array(vec!["system".to_string()]),
            None,
            &meta,
        )
        .await?;
        assert_eq!(
            2,
            system_perms.len(),
            "Should find 2 permissions with 'system' tag"
        );

        // Test 2: Filter by a single tag present in the second group of 3 permissions
        let user_perms: Vec<PermissionRow> = filter_by_value_contains(
            &ctx,
            &dbx,
            ContainsFilter::Array(vec!["user".to_string()]),
            None,
            &meta,
        )
        .await?;
        assert_eq!(
            3,
            user_perms.len(),
            "Should find 3 permissions with 'user' tag"
        );

        // Test 3: Filter for all tags in the first group to test '@>' functionality
        let critical_system_perms: Vec<PermissionRow> = filter_by_value_contains(
            &ctx,
            &dbx,
            ContainsFilter::Array(vec!["critical".to_string(), "system".to_string()]),
            None,
            &meta,
        )
        .await?;
        assert_eq!(
            2,
            critical_system_perms.len(),
            "Should find 2 permissions with both 'critical' and 'system' tags"
        );

        // Test 4: Filter by tags that do not coexist in any single record
        let no_match_perms: Vec<PermissionRow> = filter_by_value_contains(
            &ctx,
            &dbx,
            ContainsFilter::Array(vec!["system".to_string(), "general".to_string()]),
            None,
            &meta,
        )
        .await?;
        assert_eq!(
            0,
            no_match_perms.len(),
            "Should find 0 permissions with both 'system' and 'general' tags"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_ws_scoped() -> StoreResult<()> {
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx.clone());

        // 1. Setup Contexts and Data

        // Create two real workspaces to use as the scoping fixtures.
        let ctx = StoreCtx::bootstrap();
        let ws_a = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
        let ws_id_a: Uuid = ws_a.id.into();
        let ws_b = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
        let ws_id_b: Uuid = ws_b.id.into();

        // Context 1: Scoped to ws_id_a (Root context is usually used for ws_id, but we'll scope explicitly)
        let mut ctx_a = StoreCtx {
            ws_id: ws_id_a,
            ..StoreCtx::bootstrap()
        };

        // test setter method for StoreCtx
        ctx_a.set_workspace_scope(Some(ws_id_a));

        // Context 2: Scoped to ws_id_b
        let mut ctx_b = StoreCtx {
            ws_id: ws_id_b,
            ..StoreCtx::bootstrap()
        };

        ctx_b.set_workspace_scope(Some(ws_id_b));

        // Context 3: Root (Unscoped) context
        let ctx_unscoped = StoreCtx::bootstrap();
        // The actual filtering will use ctx_a, but we create data using both Uuids.

        // Helper to create permissions with a specific workspace_id and common metadata
        let c_perm = |ws_id: Uuid, i: usize| -> PermissionForCreate {
            PermissionForCreate {
                workspace_id: ws_id,
                name: format!("ws_scoped_TEST_{i}"),
                meta: PermissionMeta {
                    schema_version: "3".to_string(), // Common meta for filtering
                },
                ..Default::default()
            }
        };

        // Create 3 permissions in ws_id_a
        for i in 0..3 {
            store.create(&ctx_unscoped, c_perm(ws_id_a, i)).await?;
        }

        // Create 2 permissions in ws_id_b
        for i in 0..2 {
            store.create(&ctx_unscoped, c_perm(ws_id_b, i + 3)).await?;
        }

        // Total records created: 5 (3 in A, 2 in B).

        // 2. Define Query Metadata (filtering on 'meta' column)
        let meta = ContainsFilterQueryMeta {
            table: PermissionIden::Table,
            col: PermissionIden::Meta,
            has_audit: true,
        };

        let filter_value = ContainsFilter::Json(json!({"schema_version":"3"}));

        // 3. Test Scoped Filter (ws_id_a)

        // Expected: 3 results (Only permissions from ws_id_a should be returned)
        let results_a: Vec<PermissionRow> = filter_by_value_contains(
            &ctx_a, // Scoped context
            &dbx,
            filter_value.clone(),
            None,
            &meta,
        )
        .await?;

        assert_eq!(
            3,
            results_a.len(),
            "Scoped query for ws_id_a should return 3 records."
        );
        // Verify all returned records belong to ws_id_a
        assert!(
            results_a.iter().all(|r| r.workspace_id == ws_id_a),
            "All returned records must belong to ws_id_a."
        );

        // 4. Test Scoped Filter (ws_id_b)

        // Expected: 2 results (Only permissions from ws_id_b should be returned)
        let results_b: Vec<PermissionRow> = filter_by_value_contains(
            &ctx_b, // Scoped context
            &dbx,
            filter_value.clone(),
            None,
            &meta,
        )
        .await?;

        assert_eq!(
            2,
            results_b.len(),
            "Scoped query for ws_id_b should return 2 records."
        );
        // Verify all returned records belong to ws_id_b
        assert!(
            results_b.iter().all(|r| r.workspace_id == ws_id_b),
            "All returned records must belong to ws_id_b."
        );

        // 5. Test Unscoped Filter (to ensure all data is there)

        // Expected: 5 results (All records with schema_version: 3)
        let results_unscoped: Vec<PermissionRow> = filter_by_value_contains(
            &ctx_unscoped, // Unscoped context
            &dbx,
            filter_value,
            None,
            &meta,
        )
        .await?;

        assert_eq!(
            5,
            results_unscoped.len(),
            "Unscoped query should return all 5 records."
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_with_list_options() -> StoreResult<()> {
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = PermissionStore::new(dbx.clone());
        let ctx = StoreCtx::bootstrap(); // Use unscoped context for simplicity

        // FK prerequisite: permissions reference a real workspace.
        let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
        let ws_id: Uuid = ws.id.into();

        // 1. Setup Data

        // Helper to create permissions with different 'name' values for ordering
        let c_perm = |i: u32, name_prefix: &str| -> PermissionForCreate {
            let name = format!("{}_{:03}", name_prefix, i);

            PermissionForCreate {
                workspace_id: ws_id,
                name, // e.g., "P_005", "P_001"
                meta: PermissionMeta {
                    schema_version: "test_opts".to_string(), // Common meta for filtering
                },
                ..Default::default()
            }
        };

        // Create 5 permissions, all matching the filter, with specific names for sorting
        let names_to_create = vec![5, 1, 3, 4, 2];
        for i in names_to_create {
            store.create(&ctx, c_perm(i, "P")).await?;
        }

        // Total records created: 5.

        // 2. Define Query Metadata and Filter Value (all 5 records match)
        let meta = ContainsFilterQueryMeta {
            table: PermissionIden::Table,
            col: PermissionIden::Meta,
            has_audit: false, // Assuming false for this test
        };

        let filter_value = ContainsFilter::Json(json!({"schema_version":"test_opts"}));

        // 3. Test Ordering (Descending by 'name')

        let opts_desc = Some(ListOptions {
            limit: Some(5),
            offset: None,
            order_bys: Some(OrderBys::new(vec![OrderBy::Desc("name".to_string())])),
            ..Default::default()
        });

        let results_desc: Vec<PermissionRow> =
            filter_by_value_contains(&ctx, &dbx, filter_value.clone(), opts_desc, &meta).await?;

        assert_eq!(5, results_desc.len(), "Should find all 5 records.");
        // Check if the results are sorted 'P_005', 'P_004', 'P_003', 'P_002', 'P_001'
        assert_eq!(
            "P_005", results_desc[0].name,
            "First result should be P_005 (DESC)"
        );
        assert_eq!(
            "P_001", results_desc[4].name,
            "Last result should be P_001 (DESC)"
        );

        // 4. Test Limit and Offset (Limit 2, Offset 1, Ascending by 'name')

        let opts_limit_offset = Some(ListOptions {
            limit: Some(2),
            offset: Some(1),
            order_bys: Some(OrderBys::new(vec![OrderBy::Asc("name".to_string())])),
            ..Default::default()
        });

        let results_limited: Vec<PermissionRow> =
            filter_by_value_contains(&ctx, &dbx, filter_value.clone(), opts_limit_offset, &meta)
                .await?;

        // Expected sort order: 'P_001', 'P_002', 'P_003', 'P_004', 'P_005'
        // Limit 2, Offset 1 should return: 'P_002' and 'P_003'

        assert_eq!(
            2,
            results_limited.len(),
            "Limit should restrict results to 2."
        );
        assert_eq!(
            "P_002", results_limited[0].name,
            "First result should be P_002 (Offset 1 on ASC list)"
        );
        assert_eq!(
            "P_003", results_limited[1].name,
            "Second result should be P_003"
        );

        Ok(())
    }
}

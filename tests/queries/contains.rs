use anyhow::Result;
use env_logger::filter;
use modql::filter::{OrderBy, OrderBys};
use sea_query::Order;
use serde_json::{from_value, json};
use serial_test::serial;

use uuid::Uuid; // Need Uuid for creating a separate workspace ID

use oxideauth::{
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
            contains::FilterByContains,
            crud::{Create, Get, List},
            meta::{ContainsFilterStore, ReadStore},
        },
    },
};

use modql::filter::ListOptions;
use oxideauth::store::error::StoreResult;
use oxideauth::store::queries::contains::*;
use oxideauth::store::queries::meta::{ContainsFilter, ContainsFilterQueryMeta};

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
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
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
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
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
    let ws_a = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id_a: Uuid = ws_a.id.into();
    let ws_b = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id_b: Uuid = ws_b.id.into();

    // Context 1: Scoped to ws_id_a (Root context is usually used for ws_id, but we'll scope explicitly)
    let mut ctx_a = StoreCtx {
        ws_id: Some(ws_id_a),
        ..StoreCtx::bootstrap()
    };

    // test setter method for StoreCtx
    ctx_a.set_workspace_scope(Some(ws_id_a));

    // Context 2: Scoped to ws_id_b
    let mut ctx_b = StoreCtx {
        ws_id: Some(ws_id_b),
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
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
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

// -----------------------------------------------------------------------------
// list_with_contains / count_with_contains (combined tags + field filter)
// -----------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_list_with_tags_and_filter_combined() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = PermissionStore::new(dbx.clone());

    let mut ctx = StoreCtx::bootstrap();
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();
    ctx.set_workspace_scope(Some(ws_id));

    let name_tag_a = "LIST_TAG_AND_FILTER_A";
    let name_tag_b = "LIST_TAG_AND_FILTER_B";

    // Group A: 3 permissions tagged ["team","critical"]
    for i in 0..3 {
        let mut p = PermissionForCreate::default();
        p.workspace_id = ws_id;
        p.name = format!("{name_tag_a}_{i}");
        p.tags = vec!["team".to_string(), "critical".to_string()];
        store.create(&ctx, p).await?;
    }
    // Group B: 2 permissions tagged ["team","general"]
    for i in 0..2 {
        let mut p = PermissionForCreate::default();
        p.workspace_id = ws_id;
        p.name = format!("{name_tag_b}_{i}");
        p.tags = vec!["team".to_string(), "general".to_string()];
        store.create(&ctx, p).await?;
    }

    // Act 1: tag "team" + name contains "A" -> only group A
    let filter: PermissionFilter =
        from_value(json!({"name": {"$contains": name_tag_a}})).unwrap();
    let rows: Vec<PermissionRow> = store
        .list_with_tags_and_filter(&ctx, Some(vec!["team".to_string()]), Some(filter), None)
        .await?;
    assert_eq!(rows.len(), 3, "tag+filter must narrow to group A");

    // Act 2: tag "general" (only group B) -> group B only
    let rows: Vec<PermissionRow> = store
        .list_with_tags_and_filter(&ctx, Some(vec!["general".to_string()]), None::<PermissionFilter>, None)
        .await?;
    assert_eq!(rows.len(), 2, "only group B has the 'general' tag");

    // Act 3: no tags, no filter -> all 5 in the workspace
    let rows: Vec<PermissionRow> = store
        .list_with_tags_and_filter(&ctx, None, None::<PermissionFilter>, None)
        .await?;
    assert_eq!(rows.len(), 5, "no tags/filter must list the whole workspace");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_count_with_tags_and_filter_combined() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = PermissionStore::new(dbx.clone());

    let mut ctx = StoreCtx::bootstrap();
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();
    ctx.set_workspace_scope(Some(ws_id));

    let name_tag_a = "COUNT_TAG_AND_FILTER_A";
    let name_tag_b = "COUNT_TAG_AND_FILTER_B";

    for i in 0..3 {
        let mut p = PermissionForCreate::default();
        p.workspace_id = ws_id;
        p.name = format!("{name_tag_a}_{i}");
        p.tags = vec!["team".to_string(), "critical".to_string()];
        store.create(&ctx, p).await?;
    }
    for i in 0..2 {
        let mut p = PermissionForCreate::default();
        p.workspace_id = ws_id;
        p.name = format!("{name_tag_b}_{i}");
        p.tags = vec!["team".to_string(), "general".to_string()];
        store.create(&ctx, p).await?;
    }

    // Act & Assert
    let filter: PermissionFilter =
        from_value(json!({"name": {"$contains": name_tag_a}})).unwrap();
    let count = store
        .count_with_tags_and_filter(&ctx, Some(vec!["team".to_string()]), Some(filter))
        .await?;
    assert_eq!(count, 3, "count must match group A");

    let count = store
        .count_with_tags_and_filter(&ctx, Some(vec!["general".to_string()]), None::<PermissionFilter>)
        .await?;
    assert_eq!(count, 2, "count must match group B");

    let count = store
        .count_with_tags_and_filter(&ctx, None, None::<PermissionFilter>)
        .await?;
    assert_eq!(count, 5, "count must match the whole workspace");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_with_contains_query_fn_direct() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = PermissionStore::new(dbx.clone());

    let mut ctx = StoreCtx::bootstrap();
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();
    ctx.set_workspace_scope(Some(ws_id));

    for i in 0..4 {
        let mut p = PermissionForCreate::default();
        p.workspace_id = ws_id;
        p.name = format!("LIST_WITH_CONTAINS_DIRECT_{i}");
        p.tags = if i < 2 {
            vec!["direct-tag".to_string()]
        } else {
            vec!["other".to_string()]
        };
        store.create(&ctx, p).await?;
    }

    let meta = store.contains_tags_meta();

    // Act 1: no tags, no filter -> all rows in scope
    let rows: Vec<PermissionRow> = list_with_contains(&ctx, &dbx, None, None, None, &meta).await?;
    assert_eq!(rows.len(), 4);

    // Act 2: tag containment only
    let rows: Vec<PermissionRow> =
        list_with_contains(&ctx, &dbx, Some(vec!["direct-tag".to_string()]), None, None, &meta).await?;
    assert_eq!(rows.len(), 2, "only the two 'direct-tag' rows must match");

    // Act 3: field-based filter only (FilterGroups)
    let filter: PermissionFilter =
        from_value(json!({"name": {"$contains": "DIRECT"}})).unwrap();
    let rows: Vec<PermissionRow> = list_with_contains(&ctx, &dbx, None, Some(filter.into()), None, &meta).await?;
    assert_eq!(rows.len(), 4, "all names contain 'DIRECT'");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_count_with_contains_query_fn_direct() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = PermissionStore::new(dbx.clone());

    let mut ctx = StoreCtx::bootstrap();
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();
    ctx.set_workspace_scope(Some(ws_id));

    for i in 0..4 {
        let mut p = PermissionForCreate::default();
        p.workspace_id = ws_id;
        p.name = format!("COUNT_WITH_CONTAINS_DIRECT_{i}");
        p.tags = if i < 2 {
            vec!["direct-tag".to_string()]
        } else {
            vec!["other".to_string()]
        };
        store.create(&ctx, p).await?;
    }

    let meta = store.contains_tags_meta();

    // Act & Assert
    let count = count_with_contains(&ctx, &dbx, Some(vec!["direct-tag".to_string()]), None, &meta).await?;
    assert_eq!(count, 2);

    let filter: PermissionFilter =
        from_value(json!({"name": {"$contains": "DIRECT"}})).unwrap();
    let count = count_with_contains(&ctx, &dbx, None, Some(filter.into()), &meta).await?;
    assert_eq!(count, 4);

    Ok(())
}

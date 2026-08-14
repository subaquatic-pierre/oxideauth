use std::sync::Arc;

use anyhow::Result;
use serde_json::{from_value, json};
use serial_test::serial;
use uuid::Uuid;

use oxideauth::store::entities::id::DbId;
use oxideauth::store::entities::permission::{
    PermissionFilter, PermissionForCreate, PermissionRow,
};
use oxideauth::store::entities::workspace::WorkspaceForCreate;
use oxideauth::store::stores::workspace::WorkspaceStore;
use oxideauth::store::traits::meta::MutateStore;

use oxideauth::{
    dev::init::init_test,
    store::{
        contains::FilterByContains,
        ctx::StoreCtx,
        entities::{
            account::{AccountFilter, AccountForCreate, AccountRow},
            permission::PermissionIden,
        },
        meta::ContainsFilterStore,
        queries::crud::create,
        stores::{account::AccountStore, permission::PermissionStore},
        traits::{
            crud::{Create, Get},
            meta::ReadStore,
        },
    },
};

use oxideauth::store::dbx::PgDbx;
use oxideauth::store::error::StoreResult;
use oxideauth::store::queries::count::*;
use oxideauth::store::queries::meta::{ContainsFilter, CountManyQueryMeta};

#[tokio::test]
#[serial]
async fn test_count_after_inserts_matches_number_of_rows() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let mut ac = |i: usize| {
        let mut data = AccountForCreate::default();
        data.email = format!("user{i}{i}{i}@example.com");
        data.avatar_url = Some(format!("TEST_FILTER"));
        data
    };

    // Create N accounts
    let n = 3usize;
    let mut created: Vec<AccountRow> = Vec::with_capacity(n);
    for i in 0..n {
        let row = ac(i);
        created.push(acc_store.create(&ctx, row).await?);
    }

    // Verify via get()
    for r in &created {
        let found = acc_store.get(&ctx, &r.id).await?;
        assert_eq!(found.id, r.id);
    }

    let filter: AccountFilter =
        from_value(json!({"avatar_url":{"$contains":"TEST_FILTER"}})).unwrap();

    // Generate meta and call count with the correct arguments
    let meta = acc_store.read_meta();
    let total = count(&ctx, &dbx, Some(filter), &meta).await?;
    assert_eq!(total as usize, n);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_count_contains_matches_array_count() -> StoreResult<()> {
    // 1. Setup
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    // Assuming a store that manages entities with a 'tags' array column
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    // Define the common array of tags to check for containment
    let test_tags = vec!["premium".to_string(), "active".to_string()];
    let unique_tag = "test_tag_contains".to_string(); // Used for uniqueness

    let mut ac = |i: usize| {
        let mut data = AccountForCreate::default();
        // Ensure unique email
        data.email = format!("contains_user{i}@example.com");
        data.tags = vec![
            test_tags[0].clone(),
            test_tags[1].clone(),
            unique_tag.clone(),
        ];
        data
    };

    // 2. Create N accounts, all containing the 'test_tags'
    let n = 4usize;
    let mut created: Vec<AccountRow> = Vec::with_capacity(n);
    for i in 0..n {
        let row = ac(i);
        created.push(acc_store.create(&ctx, row).await?);
    }

    // 3. Prepare the ContainsFilter and Query Meta

    // Create the filter for array containment
    let contains_filter = ContainsFilter::Array(test_tags);

    let meta = acc_store.contains_tags_meta();

    let total = count_contains(&ctx, &dbx, contains_filter, &meta).await?;

    // All 4 created accounts should contain the tags, so the count must be 4
    assert_eq!(
        total as usize, n,
        "The count_contains result should match the number of inserted rows."
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_count_contains_partial_and_missing_tags() -> StoreResult<()> {
    // 1. Setup
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    // Define the required tags for the count check (the subset we are looking for)
    let required_tags = vec!["test_here".to_string(), "test_again".to_string()];

    // --- Create Test Data ---

    // A. Fully Contained Accounts (EXPECTED COUNT = 2)
    // These accounts contain ALL required_tags plus others.
    let fully_contained_count = 2usize;
    for i in 0..fully_contained_count {
        let mut data = AccountForCreate::default();
        data.email = format!("contained_user_{}@example.com", i);
        // Includes BOTH "premium" and "active"
        data.tags = vec![
            required_tags[0].clone(),
            required_tags[1].clone(),
            "extra".to_string(),
        ];
        acc_store.create(&ctx, data).await?;
    }

    // B. Partially Contained Account (EXCLUDED)
    // Missing "active" tag. Should NOT be counted.
    let mut data_partial = AccountForCreate::default();
    data_partial.email = "partial_user@example.com".to_string();
    data_partial.tags = vec![required_tags[0].clone(), "basic".to_string()]; // Only has "premium"
    acc_store.create(&ctx, data_partial).await?;

    // C. Empty or Irrelevant Tags Account (EXCLUDED)
    // Has none of the required tags. Should NOT be counted.
    let mut data_irrelevant = AccountForCreate::default();
    data_irrelevant.email = "irrelevant_user@example.com".to_string();
    data_irrelevant.tags = vec!["unrelated".to_string()];
    acc_store.create(&ctx, data_irrelevant).await?;

    // 2. Prepare the ContainsFilter and Query Meta

    // The filter looks for the full set of required tags
    let contains_filter = ContainsFilter::Array(required_tags);

    let meta = acc_store.contains_tags_meta();

    // 3. Call count_contains and Verify
    let total = count_contains(&ctx, &dbx, contains_filter, &meta).await?;

    // Only the 2 fully contained accounts should be counted.
    assert_eq!(
        total as usize, fully_contained_count,
        "The count_contains result should match ONLY the fully contained rows (Expected: 2)."
    );

    let contains_filter = ContainsFilter::Array(vec!["test_here".to_string()]);

    let total = count_contains(&ctx, &dbx, contains_filter, &meta).await?;
    assert_eq!(total as usize, 3, "Should include 3");

    Ok(())
}

// Helper to create a row in a specific workspace
async fn create_permission_in_ws(
    store: &PermissionStore<PgDbx>,
    ws_id: Uuid,
    name: &str,
) -> StoreResult<PermissionRow> {
    let ctx = StoreCtx::bootstrap(); // Use root context for creation to simplify
    let mut data = PermissionForCreate::default();
    data.workspace_id = ws_id;
    data.name = name.to_string();
    store.create(&ctx, data).await
}

// --- New Workspace Scoping Tests ---

#[tokio::test]
#[serial]
async fn test_count_scoped_pass() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = PermissionStore::new(dbx.clone());
    let meta = store.read_meta();

    // 1. Define two distinct workspace IDs
    let ws_store = WorkspaceStore::new(dbx.clone());
    let root_ctx = StoreCtx::bootstrap();
    let ws_a_row = ws_store.create(&root_ctx, WorkspaceForCreate::default()).await?;
    let ws_b_row = ws_store.create(&root_ctx, WorkspaceForCreate::default()).await?;
    let ws_a: Uuid = ws_a_row.id.into();
    let ws_b: Uuid = ws_b_row.id.into();
    let tag = "COUNT_SCOPED_PASS";

    // 2. Create 3 entities in WS_A (Target)
    for i in 0..3 {
        create_permission_in_ws(&store, ws_a, &format!("{tag}_A_{i}")).await?;
    }

    // 3. Create 2 entities in WS_B (Ignored)
    for i in 0..2 {
        create_permission_in_ws(&store, ws_b, &format!("{tag}_B_{i}")).await?;
    }

    // 4. Create scoped context for WS_A
    let mut scoped_ctx = StoreCtx::new(Uuid::new_v4(), ws_a);
    scoped_ctx.set_workspace_scope(Some(ws_a));

    // Filter for the tag (which exists in both A and B), but the scope should restrict the count.
    let filter: PermissionFilter = from_value(json!({"name":{"$contains": tag}})).unwrap();

    // Act
    // The count function should internally AND the user's filter with 'workspace_id = WS_A'
    let count_a = count(&scoped_ctx, &dbx, Some(filter), &meta).await?;

    // Assert
    assert_eq!(
        count_a, 3,
        "Scoped count must only include entities in WS_A."
    );

    // Clean-up/Sanity Check: Count as root (should be 5)
    let root_ctx = StoreCtx::bootstrap();
    let filter_all: PermissionFilter = from_value(json!({"name":{"$contains": tag}})).unwrap();
    let total_root = count(&root_ctx, &dbx, Some(filter_all), &meta).await?;
    assert_eq!(total_root, 5, "Root count must include all entities.");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_count_scoped_fail_other_ws() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = PermissionStore::new(dbx.clone());
    let meta = store.read_meta();

    // 1. Define workspace IDs
    let ws_store = WorkspaceStore::new(dbx.clone());
    let root_ctx = StoreCtx::bootstrap();
    let ws_enforced_row = ws_store.create(&root_ctx, WorkspaceForCreate::default()).await?;
    let ws_target_row = ws_store.create(&root_ctx, WorkspaceForCreate::default()).await?;
    let ws_enforced: Uuid = ws_enforced_row.id.into(); // User's actual scope
    let ws_target: Uuid = ws_target_row.id.into(); // Target workspace user is trying to count
    let tag = "COUNT_SCOPED_FAIL";

    // 2. Create 3 entities in WS_Target
    for i in 0..3 {
        create_permission_in_ws(&store, ws_target, &format!("{tag}_{i}")).await?;
    }

    // 3. Create scoped context for WS_ENFORCED
    let mut scoped_ctx = StoreCtx::new(Uuid::new_v4(), ws_enforced);
    scoped_ctx.set_workspace_scope(Some(ws_enforced));

    // 4. Filter attempts to explicitly count the other workspace (WS_Target)
    let filter: PermissionFilter =
        from_value(json!({"name":{"$contains": tag}, "workspace_id": ws_target.to_string()}))
            .unwrap();

    // Act
    // The query will be: WHERE (workspace_id = WS_ENFORCED) AND (workspace_id = WS_TARGET)
    let count_fail = count(&scoped_ctx, &dbx, Some(filter), &meta).await?;

    // Assert
    assert_eq!(
        count_fail, 0,
        "Scoped count must be 0 when user tries to filter for a different workspace ID."
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_count_many_scoped_pass() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = PermissionStore::new(dbx.clone());

    let ws_store = WorkspaceStore::new(dbx.clone());

    // Use AccountStore and PermissionStore to simulate a parent-child relationship
    // where Account is the parent, and Permission has a FK to Account (simulated)
    let acc_store = AccountStore::new(dbx.clone());
    let acc_mutate_meta = acc_store.mutate_meta();

    let root_ctx = StoreCtx::bootstrap();

    // 1. Define two distinct workspace IDs
    let new_ws_a = WorkspaceForCreate {
        ..Default::default()
    };
    let new_ws_b = WorkspaceForCreate {
        ..Default::default()
    };
    let ws_a = ws_store.create(&root_ctx, new_ws_a).await?;
    let ws_b = ws_store.create(&root_ctx, new_ws_b).await?;

    // 2. Create a parent account in WS_A (The target parent ID)
    let mut parent_dto = AccountForCreate::default();
    parent_dto.email = "parent_ws_a@example.com".to_string();
    let parent_a: AccountRow = create(&root_ctx, &dbx, parent_dto, &acc_mutate_meta).await?;

    // 3. Create 3 child permissions linked to Parent A in WS_A (Target)
    for i in 0..3 {
        let _ =
            create_permission_in_ws(&store, ws_a.id.into(), &format!("CHILD_A_{i}")).await?;
    }

    // 4. Create 2 child permissions linked to Parent A in WS_B (Ignored by scope)
    for i in 0..2 {
        let _ =
            create_permission_in_ws(&store, ws_b.id.into(), &format!("CHILD_B_{i}")).await?;
    }

    let parent_id: DbId = ws_a.id.into();

    // 5. Create a CountManyQueryMeta using 'name' as a simulated FK column
    // We use the tag COUNT_MANY_WS as the filter value (the parent ID)
    let count_many_meta = CountManyQueryMeta {
        table: PermissionIden::Table,
        fk: PermissionIden::WorkspaceId, // Simulating FK column name = Name
    };

    // 6. Create scoped context for WS_A
    let mut scoped_ctx = StoreCtx::new(Uuid::new_v4(), ws_a.id.into());
    scoped_ctx.set_workspace_scope(Some(ws_a.id.into()));

    // Act
    // This query counts permissions WHERE name = 'CHILD_A/B...' AND workspace_id = WS_A
    let count_a = count_many(&scoped_ctx, &dbx, &parent_id, &count_many_meta).await?;

    assert_eq!(count_a, 3);

    // Re-run the count with a root context (should be the same if no other permissions exist)
    let parent_id: DbId = ws_b.id.into();
    let root_ctx = StoreCtx::bootstrap();
    let total_root = count_many(&root_ctx, &dbx, &parent_id, &count_many_meta).await?;
    assert_eq!(total_root, 2);

    Ok(())
}

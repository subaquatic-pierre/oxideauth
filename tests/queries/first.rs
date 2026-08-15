use serde_json::{from_value, json};
use serial_test::serial;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use oxideauth::{
    dev::init::init_test,
    store::{
        ctx::StoreCtx,
        entities::{
            account::{AccountFilter, AccountForCreate, AccountRow},
            permission::{PermissionFilter, PermissionForCreate, PermissionRow},
            workspace::WorkspaceForCreate,
        },
        error::StoreError,
        queries::first::{first, first_opt},
        stores::{account::AccountStore, permission::PermissionStore},
        traits::{
            crud::{Create, GetFirst},
            meta::{MutateStore, ReadStore},
        },
    },
};

use oxideauth::store::error::StoreResult;
use modql::filter::ListOptions;

#[tokio::test]
#[serial]
async fn test_first_opt_finds_row() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let name_tag = "FIRST_FINDS_ROW";
    for i in 0..3 {
        let mut d = AccountForCreate::default();
        d.email = format!("first-finds-{i}@example.com");
        d.name = name_tag.to_string();
        store.create(&ctx, d).await?;
    }

    let filter: AccountFilter = from_value(json!({"name": {"$eq": name_tag}})).unwrap();

    // Act
    let found: Option<AccountRow> = store.first_opt(&ctx, Some(filter), None).await?;

    // Assert
    let found = found.expect("a matching row should exist");
    assert_eq!(found.name, name_tag);
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_first_opt_not_found_returns_none() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let filter: AccountFilter =
        from_value(json!({"name": {"$eq": "NON_EXISTENT_FIRST_FILTER"}})).unwrap();

    // Act
    let found: Option<AccountRow> = store.first_opt(&ctx, Some(filter), None).await?;

    // Assert
    assert!(found.is_none(), "expected Ok(None) for a non-matching filter");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_first_returns_row() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let name_tag = "FIRST_REQUIRES_ROW";
    let mut d = AccountForCreate::default();
    d.email = "first-requires-row@example.com".to_string();
    d.name = name_tag.to_string();
    let created: AccountRow = store.create(&ctx, d).await?;

    let filter: AccountFilter = from_value(json!({"name": {"$eq": name_tag}})).unwrap();

    // Act
    let found: AccountRow = store.first(&ctx, Some(filter), None).await?;

    // Assert
    assert_eq!(found.id, created.id);
    assert_eq!(found.email, created.email);
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_first_not_found_returns_entity_not_found() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let filter: AccountFilter =
        from_value(json!({"name": {"$eq": "NON_EXISTENT_FIRST_FILTER"}})).unwrap();

    // Act
    let err = store.first(&ctx, Some(filter), None).await;

    // Assert
    assert!(matches!(err, Err(StoreError::EntityNotFound { .. })));
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_first_default_order_by_created_at_desc() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let name_tag = "FIRST_ORDER_CREATED_AT";

    let mut d1 = AccountForCreate::default();
    d1.email = "first-order-1@example.com".to_string();
    d1.name = name_tag.to_string();
    let older: AccountRow = store.create(&ctx, d1).await?;

    // Ensure a distinct created_at so the ordering is deterministic.
    sleep(Duration::from_millis(20)).await;

    let mut d2 = AccountForCreate::default();
    d2.email = "first-order-2@example.com".to_string();
    d2.name = name_tag.to_string();
    let newer: AccountRow = store.create(&ctx, d2).await?;

    let filter: AccountFilter = from_value(json!({"name": {"$eq": name_tag}})).unwrap();

    // Act: no opts -> first_opt must apply a deterministic default ordering
    // (created_at DESC), returning the most recently created row.
    let found: AccountRow = store.first(&ctx, Some(filter), None).await?;

    // Assert
    assert_ne!(older.id, newer.id);
    assert_eq!(
        found.id, newer.id,
        "with no order_bys the newest row (created_at DESC) should be returned first"
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_first_explicit_order_by_name() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let name_tag = "FIRST_EXPLICIT_ORDER";
    for (i, name) in ["zzz", "aaa", "mmm"].iter().enumerate() {
        let mut d = AccountForCreate::default();
        d.email = format!("first-explicit-{i}@example.com");
        d.name = format!("{name_tag}_{name}");
        store.create(&ctx, d).await?;
    }

    let filter: AccountFilter = from_value(json!({"name": {"$contains": name_tag}})).unwrap();

    let opts = ListOptions {
        limit: Some(5),
        offset: None,
        order_bys: Some(vec!["name".to_string()].into()),
    };

    // Act
    let found: AccountRow = store.first(&ctx, Some(filter), Some(opts)).await?;

    // Assert: ASC by name -> "aaa" must be first
    assert_eq!(found.name, format!("{name_tag}_aaa"));
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_first_workspace_scoped() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = PermissionStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    let tag = "FIRST_WS_SCOPED";
    for i in 0..3 {
        let mut p = PermissionForCreate::default();
        p.workspace_id = ws_id;
        p.name = format!("{tag}_{i}");
        store.create(&ctx, p).await?;
    }

    let mut scoped_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);
    scoped_ctx.set_workspace_scope(Some(ws_id));

    let filter: PermissionFilter = from_value(json!({"name": {"$contains": tag}})).unwrap();

    // Act
    let found: PermissionRow = store.first(&scoped_ctx, Some(filter), None).await?;

    // Assert
    assert_eq!(found.workspace_id, ws_id);

    // Scoped to the workspace but the filter targets another workspace -> not found.
    let other_ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let other_ws_id: Uuid = other_ws.id.into();
    assert_ne!(ws_id, other_ws_id);

    let mut other_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);
    other_ctx.set_workspace_scope(Some(ws_id));

    let filter_other: PermissionFilter = from_value(json!({
        "name": {"$contains": tag},
        "workspace_id": other_ws_id.to_string()
    }))
    .unwrap();

    let err = store.first(&other_ctx, Some(filter_other), None).await;
    assert!(
        matches!(err, Err(StoreError::EntityNotFound { .. })),
        "scoped first must not leak rows from another workspace"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_first_query_fn_direct() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let name_tag = "FIRST_FN_DIRECT";
    for i in 0..2 {
        let mut d = AccountForCreate::default();
        d.email = format!("first-fn-direct-{i}@example.com");
        d.name = name_tag.to_string();
        store.create(&ctx, d).await?;
    }

    let meta = store.read_meta();

    // Act: first_opt (direct query function)
    let filter: AccountFilter = from_value(json!({"name": {"$eq": name_tag}})).unwrap();
    let found: Option<AccountRow> = first_opt(&ctx, &dbx, Some(filter), None, &meta).await?;
    assert!(found.is_some(), "first_opt should find a row");

    // Act: first (direct query function)
    let filter: AccountFilter = from_value(json!({"name": {"$eq": name_tag}})).unwrap();
    let row: AccountRow = first(&ctx, &dbx, Some(filter), None, &meta).await?;
    assert_eq!(row.name, name_tag);

    Ok(())
}

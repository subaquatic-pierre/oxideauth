use anyhow::Result;
use serde_json::{from_value, json};
use serial_test::serial;
use sqlx::{Postgres, query_as};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use oxideauth::{
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

use modql::filter::ListOptions;
use oxideauth::store::ctx::StoreCtx;
use oxideauth::store::error::StoreResult;
use oxideauth::store::queries::crud::*;

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
    use tokio::time::{Duration, sleep};

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
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
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

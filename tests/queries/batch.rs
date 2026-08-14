use serde_json::json;
use serial_test::serial;
use sqlx::{Postgres, query_as};
use uuid::Uuid;

use oxideauth::{
    dev::init::init_test,
    store::{
        entities::{
            account::{
                AccountFilter, AccountForCreate, AccountForUpdate, AccountMeta, AccountRow,
            },
            id::DbId,
        },
        error::StoreResult,
        queries::{
            crud::{create, list},
            meta::{MutateQueryMeta, ReadQueryMeta},
        },
        stores::account::AccountStore,
        traits::{
            crud::Get,
            meta::{MutateStore, ReadStore, TableIden},
        },
    },
};

use oxideauth::store::ctx::StoreCtx;
use oxideauth::store::queries::batch::*;

#[tokio::test]
#[serial]
async fn test_update_many() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let mutate_meta = acc_store.mutate_meta();
    let data = AccountForCreate::default();
    let ret1: AccountRow = create(&ctx, &dbx, data, &mutate_meta).await?;

    let mut data = AccountForCreate::default();
    data.email = "change".to_string();
    let ret2: AccountRow = create(&ctx, &dbx, data, &mutate_meta).await?;

    let c1 = AccountForUpdate::default();
    let mut c2 = AccountForUpdate::default();
    c2.name = None;

    let data = vec![(ret1.id, c1), (ret2.id, c2)];

    let mutate_meta = acc_store.mutate_meta();
    let r: Vec<AccountRow> = update_many(&ctx, &dbx, data, &mutate_meta).await?;

    assert_eq!(r.len(), 2, "Should return the two updated rows");
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_update_many_ignore_unknown() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let n = 3usize;
    let mut payloads: Vec<AccountForCreate> = Vec::with_capacity(n);
    let desc = "TEST_CREATE_MANY_ONE_NOT_CHANGE".to_string();

    for i in 0..n {
        let mut ac = AccountForCreate::default();
        ac.email = format!("bulk{:02}{i}@example.com", i);
        ac.description = Some(desc.clone());
        payloads.push(ac);
    }

    let mutate_meta = acc_store.mutate_meta();
    let created: Vec<AccountRow> = create_many(&ctx, &dbx, payloads, &mutate_meta).await?;
    let mut data = vec![];
    for (i, acc) in created.iter().enumerate() {
        let mut new_update = AccountForUpdate::default();
        new_update.description = Some("UPDATED DESCRIPTION".to_string());
        data.push((acc.id.clone(), new_update));
        if i == 1 {
            break;
        }
    }

    data.push((Uuid::new_v4().into(), AccountForUpdate::default()));

    let mutate_meta = acc_store.mutate_meta();
    let r: Vec<AccountRow> = update_many(&ctx, &dbx, data, &mutate_meta).await?;

    let filter: AccountFilter = json!({"description":Some(desc.clone())}).try_into()?;

    let read_meta = acc_store.read_meta();
    let found: Vec<AccountRow> = list(&ctx, &dbx, Some(filter), None, &read_meta).await?;

    assert_eq!(found.len(), 1);
    assert_eq!(r.len(), 2);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_create_many() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let n = 3usize;
    let mut payloads: Vec<AccountForCreate> = Vec::with_capacity(n);
    for i in 0..n {
        let mut ac = AccountForCreate::default();
        ac.email = format!("bulk{:02}@example.com", i);
        payloads.push(ac);
    }

    let meta = store.mutate_meta();
    let created: Vec<AccountRow> = create_many(&ctx, &dbx, payloads, &meta).await?;

    assert_eq!(created.len(), n);

    for (i, row) in created.iter().enumerate() {
        assert_eq!(row.email, format!("bulk{:02}@example.com", i));
        let fetched = store.get(&ctx, &row.id).await?;
        assert_eq!(fetched.id, row.id);
        assert_eq!(fetched.email, row.email);
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_update_many_tags() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let mutate_meta = store.mutate_meta();
    let mut c1 = AccountForCreate::default();
    c1.email = "bulk-tags-1@example.com".to_string();
    let a1: AccountRow = create(&ctx, &dbx, c1, &mutate_meta).await?;

    let mut c2 = AccountForCreate::default();
    c2.email = "bulk-tags-2@example.com".to_string();
    let a2: AccountRow = create(&ctx, &dbx, c2, &mutate_meta).await?;

    let upd1 = {
        let mut u = AccountForUpdate::default();
        u.tags = Some(vec!["alpha".into(), "beta".into()]);
        u
    };
    let upd2 = {
        let mut u = AccountForUpdate::default();
        u.tags = Some(vec!["gamma".into()]);
        u
    };

    let mutate_meta = store.mutate_meta();
    let _updated: Vec<AccountRow> =
        update_many(&ctx, &dbx, vec![(a1.id, upd1), (a2.id, upd2)], &mutate_meta).await?;

    let f1 = store.get(&ctx, &a1.id).await?;
    assert_eq!(f1.tags, ["alpha", "beta"]);

    let f2 = store.get(&ctx, &a2.id).await?;
    assert_eq!(f2.tags, ["gamma"]);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_update_many_meta() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let mutate_meta = store.mutate_meta();
    let mut c1 = AccountForCreate::default();
    c1.email = "bulk-meta-1@example.com".to_string();
    let a1: AccountRow = create(&ctx, &dbx, c1, &mutate_meta).await?;

    let mut c2 = AccountForCreate::default();
    c2.email = "bulk-meta-2@example.com".to_string();
    let a2: AccountRow = create(&ctx, &dbx, c2, &mutate_meta).await?;

    let upd1 = {
        let mut u = AccountForUpdate::default();
        u.meta = Some(AccountMeta {
            schema_version: "v1.2.3".into(),
            ..Default::default()
        });
        u
    };
    let upd2 = {
        let mut u = AccountForUpdate::default();
        u.meta = Some(AccountMeta {
            schema_version: "v9.9.9".into(),
            ..Default::default()
        });
        u
    };
    let mutate_meta = store.mutate_meta();
    let _updated: Vec<AccountRow> =
        update_many(&ctx, &dbx, vec![(a1.id, upd1), (a2.id, upd2)], &mutate_meta).await?;

    let f1 = store.get(&ctx, &a1.id).await?;
    assert_eq!(f1.meta.schema_version, "v1.2.3");

    let f2 = store.get(&ctx, &a2.id).await?;
    assert_eq!(f2.meta.schema_version, "v9.9.9");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_create_many_fail() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let over_limit = 2000usize;
    let mut payloads: Vec<AccountForCreate> = Vec::with_capacity(over_limit);
    for i in 0..over_limit {
        let mut ac = AccountForCreate::default();
        ac.email = format!("too-many-{:04}@example.com", i);
        payloads.push(ac);
    }

    let meta = store.mutate_meta();
    let res: StoreResult<Vec<AccountRow>> = create_many(&ctx, &dbx, payloads, &meta).await;

    assert!(
        res.is_err(),
        "expected create_many to fail when exceeding the max batch size"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_update_many_fail() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let mut ac = AccountForCreate::default();
    ac.email = "update-many-fail@example.com".to_string();
    let mutate_meta = store.mutate_meta();
    let row: AccountRow = create(&ctx, &dbx, ac, &mutate_meta).await?;

    let over_limit = 2000usize;
    let mut updates: Vec<(DbId, AccountForUpdate)> = Vec::with_capacity(over_limit);
    for _ in 0..over_limit {
        let mut u = AccountForUpdate::default();
        u.name = Some("bulk-name".to_string());
        updates.push((row.id, u));
    }

    let mutate_meta = store.mutate_meta();
    let res: StoreResult<Vec<AccountRow>> =
        update_many(&ctx, &dbx, updates, &mutate_meta).await;

    assert!(
        res.is_err(),
        "expected update_many to fail when exceeding the max batch size"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_delete_many() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let mutate_meta = store.mutate_meta();
    let mut mk = |i: usize| {
        let mut c = AccountForCreate::default();
        c.email = format!("del-many-{i}@example.com");
        c
    };
    let a1: AccountRow = create(&ctx, &dbx, mk(1), &mutate_meta).await?;
    let a2: AccountRow = create(&ctx, &dbx, mk(2), &mutate_meta).await?;
    let a3: AccountRow = create(&ctx, &dbx, mk(3), &mutate_meta).await?;

    let mutate_meta = store.mutate_meta();
    let deleted: Vec<AccountRow> =
        delete_many(&ctx, &dbx, vec![a1.id, a2.id, a3.id], &mutate_meta).await?;

    assert_eq!(deleted.len(), 3);

    use oxideauth::store::error::StoreError;
    for id in [a1.id, a2.id, a3.id] {
        let got = store.get(&ctx, &id).await;
        assert!(matches!(got, Err(StoreError::EntityNotFound { .. })));
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_delete_many_wrong_id() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let mut c = AccountForCreate::default();
    c.email = "del-many-wrong@example.com".into();
    let mutate_meta = store.mutate_meta();
    let a: AccountRow = create(&ctx, &dbx, c, &mutate_meta).await?;

    let wrong = Uuid::new_v4();

    let mutate_meta = store.mutate_meta();
    let deleted: Vec<AccountRow> =
        delete_many(&ctx, &dbx, vec![a.id, wrong.into()], &mutate_meta).await?;

    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].id, a.id);

    use oxideauth::store::error::StoreError;
    let got = store.get(&ctx, &a.id).await;
    assert!(matches!(got, Err(StoreError::EntityNotFound { .. })));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_delete_many_fail() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let over_limit = 2000usize;
    let ids: Vec<Uuid> = (0..over_limit).map(|_| Uuid::new_v4()).collect();

    let meta = store.mutate_meta();
    let res: StoreResult<Vec<AccountRow>> = delete_many(&ctx, &dbx, ids, &meta).await;

    assert!(
        res.is_err(),
        "expected delete_many to fail when exceeding the max batch size"
    );

    Ok(())
}

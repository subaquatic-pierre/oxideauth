use modql::field::HasSeaFields;
use modql::filter::{FilterGroups, ListOptions};
use sea_query::{
    Alias, Asterisk, CaseStatement, Condition, Expr, IdenList, IntoValueTuple,
    PostgresQueryBuilder, Query, SeaRc, SimpleExpr, WithQuery,
};
use sea_query::{Iden, IntoIden, TableRef};
use sea_query_binder::SqlxBinder;
use sqlx::{postgres::PgRow, FromRow};
use sqlx::{query_as_with, Postgres, QueryBuilder, Value};
use uuid::Uuid;

use crate::store::dbx::PgDbx;
use crate::store::entities::workspace::WorkspaceIden;
use crate::store::error::{StoreError, StoreResult};
use crate::store::queries::meta::MutateQueryMeta;
use crate::store::traits::dbx::DbExecutor;
use crate::store::traits::meta::{Store, StoreId, StoreRow, TableIden};
use crate::store::utils::{pg_type_of, prepare_audit_fields, push_sq_value};
use crate::store::utils::{prepare_workspace_scope, ListOptionsValidator};
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

    let (sql, values) = query.build_sqlx(PostgresQueryBuilder);
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
            .and_where(Expr::col(meta.pk).eq(id.clone()))
            .returning_all();

        let (sql, vals) = query.build_sqlx(PostgresQueryBuilder);

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
        let workspace_id_expr = Expr::col(WorkspaceIden::WorkspaceId).eq(ws_id);
        query.and_where(workspace_id_expr);
    }

    query
        .from_table(meta.table)
        .and_where(Expr::col(meta.pk).is_in(ids))
        .returning_all();

    let (sql, vals) = query.build_sqlx(PostgresQueryBuilder);

    let sqlx = sqlx::query_as_with::<_, T, _>(&sql, vals);

    let ret = dbx.fetch_all(sqlx).await?;

    Ok(ret)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serial_test::serial;
    use sqlx::{query_as, Postgres};
    use uuid::Uuid;

    use crate::{
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

    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_update_many() -> StoreResult<()> {
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let acc_store = AccountStore::new(dbx.clone());
        let ctx = StoreCtx::new_root();

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
        let ctx = StoreCtx::new_root();

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
        let ctx = StoreCtx::new_root();

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
        let ctx = StoreCtx::new_root();

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
        let ctx = StoreCtx::new_root();

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
        let ctx = StoreCtx::new_root();

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
        let ctx = StoreCtx::new_root();

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
        let ctx = StoreCtx::new_root();

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

        use crate::store::error::StoreError;
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
        let ctx = StoreCtx::new_root();

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

        use crate::store::error::StoreError;
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
        let ctx = StoreCtx::new_root();

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
}

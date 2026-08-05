use oxideauth_macros::HasId;
use modql::{
    field::{HasSeaFields, SeaField, SeaFields},
    filter::{FilterGroups, ListOptions},
    SIden,
};
use sea_query::{Iden, IntoIden, IntoTableRef, TableRef};
use serde::Deserialize;
use sqlx::{postgres::PgRow, FromRow};
use std::sync::Arc;
use uuid::Uuid;

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    error::StoreResult,
    init::PgPool,
    queries::{
        batch::{create_many, delete_many, update_many},
        count::count,
        crud::{create, delete, delete_opt, get, get_opt, list, update, update_opt},
        first::{first, first_opt},
        meta::{MutateQueryMeta, ReadQueryMeta},
    },
    traits::meta::{HasId, MutateStore, ReadStore, Store, StoreRow},
    utils::prepare_audit_fields,
};

/// Trait for the **"create"** capability of a store.
///
/// This provides a standard interface for inserting a new entity into the database.
pub trait Create
where
    Self: MutateStore,
{
    /// Inserts a new row into the database and returns the fully created record.
    ///
    /// This delegates to the generic `create` query function.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context, used for auditing.
    /// * `data`: The data transfer object (DTO) used for creating the entity.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing the created `Self::Row` (the full entity).
    async fn create(
        &self,
        ctx: &StoreCtx,
        data: Self::CreateStoreParams,
    ) -> StoreResult<Self::Row> {
        let dbx = self.dbx();
        let meta = self.mutate_meta();
        create(&ctx, &dbx, data, &meta).await
    }
}

/// Trait for the **"get by id"** capability of a store.
///
/// This provides standard methods for retrieving a single entity by its primary key.
pub trait Get
where
    Self: ReadStore,
{
    /// Fetches a single row by its **primary key** (`id`).
    ///
    /// This method delegates to the generic `get` query function and **returns an error**
    /// (`StoreError::EntityNotFound`) if the row is not found.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `id`: The primary key identifier of the entity to fetch.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing the fetched entity (`Self::Row`).
    async fn get(&self, ctx: &StoreCtx, id: &<Self::Row as HasId>::Id) -> StoreResult<Self::Row> {
        let dbx = self.dbx();
        let meta = self.read_meta();
        get(&ctx, &dbx, id, &meta).await
    }

    /// Fetches a single row by its **primary key** (`id`).
    ///
    /// This method delegates to the generic `get_opt` query function and returns
    /// `Ok(None)` if the row is not found, avoiding an error throw.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `id`: The primary key identifier of the entity to fetch.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing `Some(Self::Row)` if found, or `None` otherwise.
    async fn get_opt(
        &self,
        ctx: &StoreCtx,
        id: &<Self::Row as HasId>::Id,
    ) -> StoreResult<Option<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.read_meta();
        get_opt(&ctx, &dbx, id, &meta).await
    }
}

/// Trait for the **"list/filter"** capability of a store.
///
/// This provides a standard interface for retrieving a collection of entities
/// based on optional filtering, sorting, and pagination options.
pub trait List
where
    Self: ReadStore,
{
    /// Returns all rows matching the optional filter and list options.
    ///
    /// This method delegates to the generic `list` query function.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `filter`: Optional filter parameters to narrow the result set (converts to `FilterGroups`).
    /// * `opts`: Optional pagination and sorting parameters.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing a vector of matching rows (`Vec<Self::Row>`).
    async fn list(
        &self,
        ctx: &StoreCtx,
        filter: Option<Self::FilterStoreParams>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.read_meta();
        list(ctx, &dbx, filter, opts, &meta).await
    }
}

/// Trait for the **"update"** capability of a store.
///
/// This provides standard methods for modifying an existing entity by its primary key.
pub trait Update
where
    Self: MutateStore,
{
    /// Updates a row by its **primary key** (`id`) using the provided `data` and returns the updated record.
    ///
    /// This method delegates to the generic `update` query function and **returns an error**
    /// (`StoreError::EntityNotFound`) if the row is not found.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context, used for auditing (setting `updated_by`).
    /// * `id`: The primary key identifier of the entity to update.
    /// * `data`: The DTO containing the fields to modify.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing the fully updated entity (`Self::Row`).
    async fn update(
        &self,
        ctx: &StoreCtx,
        id: &<Self::Row as HasId>::Id,
        data: Self::UpdateStoreParams,
    ) -> StoreResult<Self::Row> {
        let dbx = self.dbx();
        let meta = self.mutate_meta();
        update(ctx, &dbx, id, data, &meta).await
    }

    /// Updates a row by its **primary key** (`id`) using the provided `data` and returns the updated record.
    ///
    /// This method delegates to the generic `update_opt` query function and returns
    /// `Ok(None)` if the row was not found, avoiding an error throw.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context, used for auditing (setting `updated_by`).
    /// * `id`: The primary key identifier of the entity to update.
    /// * `data`: The DTO containing the fields to modify.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing `Some(Self::Row)` if updated, or `None` if not found.
    async fn update_opt(
        &self,
        ctx: &StoreCtx,
        id: &<Self::Row as HasId>::Id,
        data: Self::UpdateStoreParams,
    ) -> StoreResult<Option<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.mutate_meta();
        update_opt(ctx, &dbx, id, data, &meta).await
    }
}

/// Trait for the **"delete"** capability of a store.
///
/// This provides standard methods for removing an existing entity by its primary key.
pub trait Delete
where
    Self: MutateStore,
{
    /// Deletes a row by its **primary key** (`id`) and returns the deleted record.
    ///
    /// This method delegates to the generic `delete` query function and **returns an error**
    /// (`StoreError::EntityNotFound`) if the row is not found or could not be deleted.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `id`: The primary key identifier of the entity to delete.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing the deleted entity (`Self::Row`).
    async fn delete(
        &self,
        ctx: &StoreCtx,
        id: &<Self::Row as HasId>::Id,
    ) -> StoreResult<Self::Row> {
        let dbx = self.dbx();
        let meta = self.mutate_meta();
        delete(ctx, &dbx, id, &meta).await
    }

    /// Deletes a row by its **primary key** (`id`) and returns the deleted record.
    ///
    /// This method delegates to the generic `delete_opt` query function and returns
    /// `Ok(None)` if the row was not found, making the operation idempotent against
    /// missing entities.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `id`: The primary key identifier of the entity to delete.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing `Some(Self::Row)` if deleted, or `None` if not found.
    async fn delete_opt(
        &self,
        ctx: &StoreCtx,
        id: &<Self::Row as HasId>::Id,
    ) -> StoreResult<Option<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.mutate_meta();
        delete_opt(ctx, &dbx, id, &meta).await
    }
}

/// Trait for the bulk **"create"** capability of a store.
///
/// This provides a standard interface for inserting multiple entities in a single batch operation.
pub trait CreateMany
where
    Self: MutateStore,
{
    /// Inserts multiple new rows into the database and returns the created records.
    ///
    /// This delegates to the generic `create_many` query function. It typically uses a single
    /// `INSERT INTO ... VALUES (), (), () ... RETURNING *` statement for efficiency.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context, used for auditing.
    /// * `data`: A vector of data transfer objects (DTOs) used for creating the entities.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing a vector of the fully created entities (`Vec<Self::Row>`).
    async fn create_many(
        &self,
        ctx: &StoreCtx,
        data: Vec<Self::CreateStoreParams>,
    ) -> StoreResult<Vec<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.mutate_meta();
        create_many(ctx, &dbx, data, &meta).await
    }
}

/// Trait for the bulk **"update"** capability of a store.
///
/// This provides a standard interface for updating multiple existing entities in a single batch operation.
pub trait UpdateMany
where
    Self: MutateStore,
{
    /// Updates multiple rows from a vector of `(ID, data)` tuples
    /// and returns the fully updated records.
    ///
    /// This delegates to the generic `update_many` query function, typically using a single
    /// `UPDATE ... FROM (VALUES ...) ...` statement or similar upsert logic for bulk updates.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context, used for auditing (setting `updated_by`).
    /// * `data`: A vector of tuples, where each tuple contains the **ID** of the entity to update
    ///   and the **DTO** (`UpdateStoreParams`) containing the fields to modify.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing a vector of the fully updated entities (`Vec<Self::Row>`).
    async fn update_many(
        &self,
        ctx: &StoreCtx,
        data: Vec<(<Self::Row as HasId>::Id, Self::UpdateStoreParams)>,
    ) -> StoreResult<Vec<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.mutate_meta();
        update_many(ctx, &dbx, data, &meta).await
    }
}

/// Trait for the bulk **"delete"** capability of a store.
///
/// This provides a standard interface for removing multiple existing entities in a single batch operation.
pub trait DeleteMany
where
    Self: MutateStore,
{
    /// Deletes multiple rows by their primary keys and returns the deleted records.
    ///
    /// This delegates to the generic `delete_many` query function, typically using a single
    /// `DELETE FROM ... WHERE pk IN (...) RETURNING *` statement for bulk removal.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `ids`: A vector of **primary key IDs** of the entities to delete.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing a vector of the fully deleted entities (`Vec<Self::Row>`).
    /// The returned vector will only contain entities that were successfully found and deleted.
    async fn delete_many(
        &self,
        ctx: &StoreCtx,
        ids: Vec<<Self::Row as HasId>::Id>,
    ) -> StoreResult<Vec<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.mutate_meta();
        delete_many(ctx, &dbx, ids, &meta).await
    }
}

/// Trait for fetching the **first record** matching a filter.
///
/// This provides standard methods for retrieving a single entity when multiple matches
/// might exist, enforcing a deterministic order if one isn't provided.
pub trait GetFirst
where
    Self: ReadStore,
{
    /// Fetches the first row matching the optional filter and list options.
    ///
    /// This method delegates to the generic `first` query function and **returns an error**
    /// (`StoreError::EntityNotFound`) if no matching row is found. A default ordering
    /// (e.g., by creation date) is applied if no `ListOptions` sorting is provided.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `filter`: Optional filter parameters.
    /// * `opts`: Optional sorting parameters (`order_bys`). Limit is implicitly set to 1.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing the first matching entity (`Self::Row`).
    async fn first(
        &self,
        ctx: &StoreCtx,
        filter: Option<Self::FilterStoreParams>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Self::Row> {
        let dbx = self.dbx();
        let meta = self.read_meta();
        first(ctx, &dbx, filter, opts, &meta).await
    }

    /// Fetches the first row matching the optional filter and list options.
    ///
    /// This method delegates to the generic `first_opt` query function and returns
    /// `Ok(None)` if no matching row is found, avoiding an error throw. A default ordering
    /// is applied if no `ListOptions` sorting is provided.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `filter`: Optional filter parameters.
    /// * `opts`: Optional sorting parameters (`order_bys`). Limit is implicitly set to 1.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing `Some(Self::Row)` if found, or `None` otherwise.
    async fn first_opt(
        &self,
        ctx: &StoreCtx,
        filter: Option<Self::FilterStoreParams>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Option<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.read_meta();
        first_opt(ctx, &dbx, filter, opts, &meta).await
    }
}

/// Trait for **counting records** matching a filter.
///
/// This provides a standard interface for determining the total number of entities
/// that satisfy optional filter conditions.
pub trait GetCount
where
    Self: ReadStore,
{
    /// Returns a count of all rows matching the optional filter.
    ///
    /// This method delegates to the generic `count` query function, performing a
    /// `SELECT COUNT(*) FROM table WHERE condition` query.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `filter`: Optional filter parameters used to narrow the rows being counted.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing the total count as an `i64`.
    async fn count(
        &self,
        ctx: &StoreCtx,
        filter: Option<Self::FilterStoreParams>,
    ) -> StoreResult<i64> {
        let dbx = self.dbx();
        let meta = self.read_meta();
        count(ctx, &dbx, filter, &meta).await
    }
}

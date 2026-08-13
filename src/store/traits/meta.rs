//! # Store Trait Architecture
//!
//! This module defines the core traits for building a capabilities-based data access layer.
//! The design separates metadata-providing traits from functional traits that provide behavior.
//!
//! ## Core Design Pattern
//!
//! 1.  **Meta Traits (e.g., `ReadStore`, `MutateStore`):** These traits are the contracts that a specific store (like `AccountStore`) must implement. They don't contain behavior but require the store to provide essential metadata, such as table/column identifiers and associated types for creating or filtering data.
//!
//! 2.  **Functional Traits (e.g., `Get`, `List`, `Create`):** These traits define the actual data access methods (`.get()`, `.list()`, etc.). They are implemented generically for any type that fulfills the corresponding meta trait contract.
//!
//! 3.  **Blanket Implementations:** By implementing a meta trait like `ReadStore`, a store struct **automatically** gains the capabilities of `Get`, `List`, `GetFirst`, and `GetCount` without any additional boilerplate. This file centralizes these blanket `impls` directly under the meta traits they depend on.

use modql::field::HasSeaFields;
use modql::filter::{FilterGroups, ListOptions};
use sea_query::{Iden, IntoIden};
use sqlx::{FromRow, postgres::PgRow};

use crate::store::traits::dbx::DbExecutor;
use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    queries::meta::{
        ContainsFilterQueryMeta, ManyToManyQueryMeta, MutateQueryMeta, OneToManyQueryMeta,
        ReadQueryMeta,
    },
    traits::{
        contains::FilterByContains,
        crud::{
            Create, CreateMany, Delete, DeleteMany, Get, GetCount, GetFirst, List, Update,
            UpdateMany,
        },
        join::{GetManyToMany, GetOneToMany, LinkManyToMany, ListManyToMany, ListOneToMany},
    },
};

// region:    --- Core ID and Row Abstractions

/// A marker trait for table identifier enums (e.g., `AccountIden`).
pub trait TableIden: 'static + Copy + Iden + Send + Sync {}

/// A trait for types that can be used as primary keys in the store.
pub trait StoreId: ToString + Into<sea_query::Value> + Send + Sync + Clone + Copy {}

/// A trait for structs that represent a database row and have an identifiable primary key.
pub trait HasId {
    /// The type of the primary key (e.g., `Uuid`).
    type Id: StoreId;
}

/// A trait that combines `HasId` and `sqlx::FromRow` for any struct that can be mapped from a `PgRow`.
pub trait StoreRow: HasId + for<'r> FromRow<'r, PgRow> + Unpin + Send + Sync {}

// Blanket implementations for the core abstractions.
impl<T> StoreRow for T where T: HasId + for<'r> FromRow<'r, PgRow> + Unpin + Send + Sync {}
impl<T> StoreId for T where T: ToString + Into<sea_query::Value> + Send + Sync + Clone + Copy {}
impl<T: 'static + Copy + Iden + Send + Sync> TableIden for T {}

// endregion: --- Core ID and Row Abstractions

// region:    --- Store Meta Traits & Blanket Impls

/// The base trait for all stores, providing access to the database connection.
pub trait Store: Sized + Send + Sync {
    /// The identifier enum for the store's table and columns.
    type Iden: TableIden;
    /// The struct type that this store primarily returns from queries.
    type Row: StoreRow;

    /// Access the underlying database connection wrapper (`PgDbx`).
    fn dbx(&self) -> impl DbExecutor;
}

/// Requires a store to provide metadata for read operations (get, list, etc.).
pub trait ReadStore: Store {
    /// The struct used to specify filter parameters for list queries.
    type FilterStoreParams: Into<FilterGroups> + Send;
    /// Returns the metadata required to build read queries.
    fn read_meta(&self) -> ReadQueryMeta<Self::Iden>;
}
// By implementing `ReadStore`, a type automatically gains the following capabilities:
impl<T: ReadStore> Get for T {}
impl<T: ReadStore> List for T {}
impl<T: ReadStore> GetFirst for T {}
impl<T: ReadStore> GetCount for T {}

/// Requires a store to provide metadata for write operations (create, update, delete).
pub trait MutateStore: Store {
    /// The struct used to provide data for creating a new row.
    type CreateStoreParams: HasSeaFields + Send;
    /// The struct used to provide data for updating an existing row.
    type UpdateStoreParams: HasSeaFields + Clone + Send + Sync + Sized;
    /// Returns the metadata required to build write queries.
    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden>;
}
// By implementing `MutateStore`, a type automatically gains the following capabilities:
impl<T: MutateStore> Create for T {}
impl<T: MutateStore> Update for T {}
impl<T: MutateStore> Delete for T {}
impl<T: MutateStore> CreateMany for T {}
impl<T: MutateStore> UpdateMany for T {}
impl<T: MutateStore> DeleteMany for T {}

/// Requires a store to provide metadata for one-to-many relationship queries.
pub trait OneToManyStore: Store {
    /// The struct representing the "one" side with the "many" side aggregated into it.
    type OneToManyRow: StoreRow;
    /// The struct used to filter list queries on the "one" side's table.
    type FilterStoreParams: Into<FilterGroups> + Send + Clone;
    /// Returns the metadata defining the one-to-many relationship.
    fn one_to_many_meta(&self) -> OneToManyQueryMeta<Self::Iden>;
}
// By implementing `OneToManyStore`, a type automatically gains the following capabilities:
impl<T: OneToManyStore> GetOneToMany for T {}
impl<T: OneToManyStore + ReadStore> ListOneToMany for T {} // Note: `List` requires `ReadStore`

/// Requires a store to provide metadata for many-to-many relationship queries.
pub trait ManyToManyStore: Store {
    /// The struct representing one entity with its related entities aggregated into it.
    type ManyToManyRow: StoreRow;
    /// The struct used to filter list queries on the base table.
    type FilterStoreParams: Into<FilterGroups> + Send + Clone;
    /// Returns the metadata defining the many-to-many relationship.
    fn many_to_many_meta(&self) -> ManyToManyQueryMeta<Self::Iden>;
}
// By implementing `ManyToManyStore`, a type automatically gains the following capabilities:
impl<T: ManyToManyStore> GetManyToMany for T {}
impl<T: ManyToManyStore + ReadStore> ListManyToMany for T {} // Note: `List` requires `ReadStore`
impl<T: ManyToManyStore> LinkManyToMany for T {}

/// Requires a store to provide metadata for filtering on JSONB columns.
pub trait ContainsFilterStore: Store {
    /// Returns metadata for querying a JSONB array column (e.g., `tags`).
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden>;
    /// Returns metadata for querying a JSONB object column (e.g., `meta`).
    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden>;
}
// By implementing `ContainsFilterStore`, a type automatically gains the following capabilities:
impl<T: ContainsFilterStore> FilterByContains for T {}

// endregion: --- Store Meta Traits & Blanket Impls

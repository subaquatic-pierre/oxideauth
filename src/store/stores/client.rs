use std::sync::Arc;

use crate::store::{
    entities::client::{ClientFilter, ClientForCreate, ClientForUpdate, ClientIden, ClientRow},
    queries::meta::{ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta},
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Client store, holding the database connection wrapper.
pub struct ClientStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> ClientStore<D> {
    /// Creates a new `ClientStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, ClientStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for ClientStore<D> {
    type Iden = ClientIden;
    type Row = ClientRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for ClientStore<D> {
    type FilterStoreParams = ClientFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: ClientIden::Table,
            pk: ClientIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for ClientStore<D> {
    type CreateStoreParams = ClientForCreate;
    type UpdateStoreParams = ClientForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: ClientIden::Table,
            pk: ClientIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for ClientStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ClientIden::Table,
            col: ClientIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ClientIden::Table,
            col: ClientIden::Meta,
            has_audit: true,
        }
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

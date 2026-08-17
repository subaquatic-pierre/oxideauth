use std::sync::Arc;

use sea_query::Iden;
use serde_json::json;

use crate::store::{
    crud::List,
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        permission::{
            PermissionFilter, PermissionForCreate, PermissionForUpdate, PermissionIden,
            PermissionRow,
        },
    },
    error::{StoreError, StoreResult},
    meta::ManyToManyStore,
    queries::{
        batch::find_many_where_value_in_key,
        meta::{ContainsFilterQueryMeta, ManyToManyQueryMeta, MutateQueryMeta, ReadQueryMeta},
    },
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Permission store, holding the database connection wrapper.
pub struct PermissionStore<D: DbExecutor> {
    dbx: Arc<D>,
    has_audit: bool,
}

impl<D: DbExecutor> PermissionStore<D> {
    /// Creates a new `PermissionStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self {
            dbx,
            has_audit: true,
        }
    }

    pub async fn find_all_many_by_names(
        &self,
        ctx: &StoreCtx,
        names: Vec<String>,
    ) -> StoreResult<Vec<PermissionRow>> {
        let meta = ReadQueryMeta {
            table: PermissionIden::Table,
            pk: PermissionIden::Name,
            has_audit: self.has_audit,
        };

        let res = find_many_where_value_in_key(ctx, &self.dbx, names, &meta).await?;

        Ok(res)
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, PermissionStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for PermissionStore<D> {
    type Iden = PermissionIden;
    type Row = PermissionRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for PermissionStore<D> {
    type FilterStoreParams = PermissionFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: PermissionIden::Table,
            pk: PermissionIden::Id,
            has_audit: self.has_audit,
        }
    }
}

impl<D: DbExecutor> MutateStore for PermissionStore<D> {
    type CreateStoreParams = PermissionForCreate;
    type UpdateStoreParams = PermissionForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: PermissionIden::Table,
            pk: PermissionIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for PermissionStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: PermissionIden::Table,
            col: PermissionIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: PermissionIden::Table,
            col: PermissionIden::Meta,
            has_audit: true,
        }
    }
}

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------

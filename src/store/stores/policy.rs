use std::sync::Arc;

use sea_query::Iden;
use serde_json::json;

use crate::store::{
    crud::List,
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        policy::{
            PolicyFilter, PolicyForCreate, PolicyForUpdate, PolicyIden, PolicyRow,
        },
    },
    error::StoreResult,
    queries::meta::{
        ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta,
    },
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Policy store, holding the database connection wrapper.
///
/// NOTE: `runtime_key` uniqueness per workspace is **not** enforced at the
/// store layer (the key is derived, not stored). It is validated by the
/// service layer (`PolicyService`).
pub struct PolicyStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> PolicyStore<D> {
    /// Creates a new `PolicyStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    /// Looks up a policy by its workspace-scoped `name` (unique per workspace
    /// when present).
    pub async fn get_by_name_opt(
        &self,
        ctx: &StoreCtx,
        name: &str,
        workspace_id: DbId,
    ) -> StoreResult<Option<PolicyRow>> {
        let filter: PolicyFilter = json!({
            "name": name.to_string(),
            "workspace_id": workspace_id.to_string()
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, PolicyStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for PolicyStore<D> {
    type Iden = PolicyIden;
    type Row = PolicyRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for PolicyStore<D> {
    type FilterStoreParams = PolicyFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: PolicyIden::Table,
            pk: PolicyIden::Id,
            has_audit: false,
        }
    }
}

impl<D: DbExecutor> MutateStore for PolicyStore<D> {
    type CreateStoreParams = PolicyForCreate;
    type UpdateStoreParams = PolicyForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: PolicyIden::Table,
            pk: PolicyIden::Id,
            has_audit: false,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for PolicyStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: PolicyIden::Table,
            col: PolicyIden::Tags,
            has_audit: false,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: PolicyIden::Table,
            col: PolicyIden::Meta,
            has_audit: false,
        }
    }
}

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------

use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        profile::{ProfileFilter, ProfileForCreate, ProfileForUpdate, ProfileIden, ProfileRow},
    },
    error::StoreResult,
    queries::meta::{ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta},
    traits::{
        crud::List,
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Profile store, holding the database connection wrapper.
pub struct ProfileStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> ProfileStore<D> {
    /// Creates a new `ProfileStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    /// Finds the single profile for an account within a workspace (if any).
    pub async fn find_by_account_workspace(
        &self,
        ctx: &StoreCtx,
        account_id: Uuid,
        workspace_id: Uuid,
    ) -> StoreResult<Option<ProfileRow>> {
        let filter: ProfileFilter = json!({
            "account_id": account_id.to_string(),
            "workspace_id": workspace_id.to_string(),
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }

    /// Finds the single profile with the given email within a workspace (if any).
    ///
    /// The email is normalized (trimmed + lowercased) for a case-insensitive
    /// lookup. Stored emails are lowercased on write (via the core email
    /// validator), so exact equality is sufficient and aligns with the
    /// `profile_workspace_email_lower_key` unique index.
    pub async fn find_by_email_workspace(
        &self,
        ctx: &StoreCtx,
        workspace_id: Uuid,
        email: &str,
    ) -> StoreResult<Option<ProfileRow>> {
        let normalized = email.trim().to_lowercase();

        let filter: ProfileFilter = json!({
            "workspace_id": workspace_id.to_string(),
            "email": normalized,
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, ProfileStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for ProfileStore<D> {
    type Iden = ProfileIden;
    type Row = ProfileRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for ProfileStore<D> {
    type FilterStoreParams = ProfileFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: ProfileIden::Table,
            pk: ProfileIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for ProfileStore<D> {
    type CreateStoreParams = ProfileForCreate;
    type UpdateStoreParams = ProfileForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: ProfileIden::Table,
            pk: ProfileIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for ProfileStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ProfileIden::Table,
            col: ProfileIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ProfileIden::Table,
            col: ProfileIden::Meta,
            has_audit: true,
        }
    }
}

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------

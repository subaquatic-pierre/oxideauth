use std::sync::Arc;

use modql::filter::ListOptions;
use sea_query::Iden;
use serde_json::json;

use crate::store::{
    crud::List,
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        role::{
            RoleFilter, RoleForCreate, RoleForUpdate, RoleIden, RoleRow, RoleWithPermissions,
            RoleWithPolicies,
        },
    },
    error::{StoreError, StoreResult},
    queries::{
        join::{attach_link, detach_link, get_many_to_many, set_many_to_many_links},
        list::list_containing_many,
        meta::{
            ContainsFilterQueryMeta, ListContainingManyQueryMeta, ManyToManyQueryMeta,
            MutateQueryMeta, ReadQueryMeta,
        },
    },
    traits::{
        dbx::DbExecutor,
        meta::{ContainsFilterStore, ManyToManyStore, MutateStore, ReadStore, Store},
    },
};
/// The struct for our Role store, holding the database connection wrapper.
pub struct RoleStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> RoleStore<D> {
    /// Creates a new `RoleStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    pub async fn get_by_name(
        &self,
        ctx: &StoreCtx,
        name: &str,
        workspace_id: DbId,
    ) -> StoreResult<RoleRow> {
        match self.get_by_name_opt(ctx, name, workspace_id).await? {
            Some(row) => Ok(row),
            None => Err(StoreError::EntityNotFound {
                entity: self.read_meta().table.to_string(),
                id: name.to_string(),
            }),
        }
    }

    pub async fn get_by_name_opt(
        &self,
        ctx: &StoreCtx,
        name: &str,
        workspace_id: DbId,
    ) -> StoreResult<Option<RoleRow>> {
        let filter: RoleFilter = json!({
            "name": name.to_string(),
            "workspace_id": workspace_id.to_string()
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }

    /// Lists roles whose set of linked permissions **contains all** of the given
    /// permission IDs (via the `role_permission` join table).
    pub async fn list_containing_permissions(
        &self,
        ctx: &StoreCtx,
        permission_ids: Vec<DbId>,
        tags: Option<Vec<String>>,
        filter: Option<RoleFilter>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<RoleRow>> {
        let meta = ListContainingManyQueryMeta {
            table: RoleIden::Table,
            pk: RoleIden::Id,
            join_table: RoleIden::RolePermission,
            join_fk: RoleIden::RoleId,
            join_many_fk: RoleIden::PermissionId,
            has_audit: true,
        };

        list_containing_many(ctx, &self.dbx, permission_ids, tags, filter, opts, &meta).await
    }

    /// Lists roles whose set of linked policies **contains all** of the given
    /// policy IDs (via the `role_policy` join table).
    pub async fn list_containing_policies(
        &self,
        ctx: &StoreCtx,
        policy_ids: Vec<DbId>,
        tags: Option<Vec<String>>,
        filter: Option<RoleFilter>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<RoleRow>> {
        let meta = ListContainingManyQueryMeta {
            table: RoleIden::Table,
            pk: RoleIden::Id,
            join_table: RoleIden::RolePolicy,
            join_fk: RoleIden::RoleId,
            join_many_fk: RoleIden::PolicyId,
            has_audit: true,
        };

        list_containing_many(ctx, &self.dbx, policy_ids, tags, filter, opts, &meta).await
    }

    /// Fetches a single role with its linked policies aggregated (via the
    /// `role_policy` join table). Mirrors `get_many_to_many` for permissions.
    pub async fn get_many_to_many_policies(
        &self,
        ctx: &StoreCtx,
        role_id: &DbId,
    ) -> StoreResult<RoleWithPolicies> {
        let meta = self.policy_many_to_many_meta();
        get_many_to_many(ctx, &self.dbx, role_id, &meta).await
    }

    /// Replaces the role's policy links (set semantics) in the `role_policy`
    /// join table. Mirrors `set_many_to_many_links` for permissions.
    pub async fn set_many_to_many_policies(
        &self,
        ctx: &StoreCtx,
        role_id: &DbId,
        policy_ids: Vec<DbId>,
    ) -> StoreResult<()> {
        let meta = self.policy_many_to_many_meta();
        set_many_to_many_links(ctx, &self.dbx, role_id, policy_ids, &meta).await
    }

    /// Attaches a single policy to the role (idempotent).
    pub async fn attach_policy(
        &self,
        ctx: &StoreCtx,
        role_id: &DbId,
        policy_id: &DbId,
    ) -> StoreResult<()> {
        let meta = self.policy_many_to_many_meta();
        attach_link(ctx, &self.dbx, role_id, policy_id, &meta).await
    }

    /// Detaches a single policy from the role (idempotent).
    pub async fn detach_policy(
        &self,
        ctx: &StoreCtx,
        role_id: &DbId,
        policy_id: &DbId,
    ) -> StoreResult<()> {
        let meta = self.policy_many_to_many_meta();
        detach_link(ctx, &self.dbx, role_id, policy_id, &meta).await
    }

    /// Metadata for the `role_policy` many-to-many join (mirrors the
    /// `role_permission` metadata exposed by `ManyToManyStore`).
    fn policy_many_to_many_meta(&self) -> ManyToManyQueryMeta<RoleIden> {
        ManyToManyQueryMeta {
            single_table: RoleIden::Table,
            many_table: RoleIden::Policy,
            join_table: RoleIden::RolePolicy,
            single_pk: RoleIden::Id,
            many_pk: RoleIden::PolicyPk,
            many_fk: RoleIden::PolicyId,
            join_fk: RoleIden::RoleId,
            agg_alias: RoleIden::Policies,
            has_audit: true,
        }
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, RoleStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for RoleStore<D> {
    type Iden = RoleIden;
    type Row = RoleRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for RoleStore<D> {
    type FilterStoreParams = RoleFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: RoleIden::Table,
            pk: RoleIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for RoleStore<D> {
    type CreateStoreParams = RoleForCreate;
    type UpdateStoreParams = RoleForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: RoleIden::Table,
            pk: RoleIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ManyToManyStore for RoleStore<D> {
    type ManyToManyRow = RoleWithPermissions;

    type FilterStoreParams = RoleFilter;

    fn many_to_many_meta(&self) -> ManyToManyQueryMeta<Self::Iden> {
        ManyToManyQueryMeta {
            single_table: RoleIden::Table,
            many_table: RoleIden::Permission,
            join_table: RoleIden::RolePermission,
            single_pk: RoleIden::Id,
            many_pk: RoleIden::PermissionPk,
            many_fk: RoleIden::PermissionId,
            join_fk: RoleIden::RoleId,
            agg_alias: RoleIden::Permissions,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for RoleStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: RoleIden::Table,
            col: RoleIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: RoleIden::Table,
            col: RoleIden::Meta,
            has_audit: true,
        }
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------

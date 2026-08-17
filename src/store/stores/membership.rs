use std::sync::Arc;

use modql::filter::ListOptions;

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        membership::{
            MembershipFilter, MembershipForCreate, MembershipForUpdate, MembershipIden,
            MembershipRow, MembershipWithPolicies, MembershipWithRoles,
        },
    },
    error::StoreResult,
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

/// The struct for our Membership store, holding the database connection wrapper.
pub struct MembershipStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> MembershipStore<D> {
    /// Creates a new `MembershipStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    /// Lists memberships whose set of linked roles **contains all** of the given
    /// role IDs (via the `membership_role` join table).
    pub async fn list_containing_roles(
        &self,
        ctx: &StoreCtx,
        role_ids: Vec<DbId>,
        tags: Option<Vec<String>>,
        filter: Option<MembershipFilter>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<MembershipRow>> {
        let meta = ListContainingManyQueryMeta {
            table: MembershipIden::Table,
            pk: MembershipIden::Id,
            join_table: MembershipIden::MembershipRole,
            join_fk: MembershipIden::MembershipId,
            join_many_fk: MembershipIden::RoleId,
            has_audit: true,
        };

        list_containing_many(ctx, &self.dbx, role_ids, tags, filter, opts, &meta).await
    }

    /// Lists memberships whose set of linked policies **contains all** of the given
    /// policy IDs (via the `membership_policy` join table).
    pub async fn list_containing_policies(
        &self,
        ctx: &StoreCtx,
        policy_ids: Vec<DbId>,
        tags: Option<Vec<String>>,
        filter: Option<MembershipFilter>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<MembershipRow>> {
        let meta = ListContainingManyQueryMeta {
            table: MembershipIden::Table,
            pk: MembershipIden::Id,
            join_table: MembershipIden::MembershipPolicy,
            join_fk: MembershipIden::MembershipId,
            join_many_fk: MembershipIden::PolicyId,
            has_audit: true,
        };

        list_containing_many(ctx, &self.dbx, policy_ids, tags, filter, opts, &meta).await
    }

    /// Fetches a single membership with its linked policies aggregated (via the
    /// `membership_policy` join table). Mirrors `get_many_to_many` for roles.
    pub async fn get_many_to_many_policies(
        &self,
        ctx: &StoreCtx,
        membership_id: &DbId,
    ) -> StoreResult<MembershipWithPolicies> {
        let meta = self.policy_many_to_many_meta();
        get_many_to_many(ctx, &self.dbx, membership_id, &meta).await
    }

    /// Replaces the membership's policy links (set semantics) in the
    /// `membership_policy` join table. Mirrors `set_many_to_many_links` for roles.
    pub async fn set_many_to_many_policies(
        &self,
        ctx: &StoreCtx,
        membership_id: &DbId,
        policy_ids: Vec<DbId>,
    ) -> StoreResult<()> {
        let meta = self.policy_many_to_many_meta();
        set_many_to_many_links(ctx, &self.dbx, membership_id, policy_ids, &meta).await
    }

    /// Attaches a single policy to the membership (idempotent).
    pub async fn attach_policy(
        &self,
        ctx: &StoreCtx,
        membership_id: &DbId,
        policy_id: &DbId,
    ) -> StoreResult<()> {
        let meta = self.policy_many_to_many_meta();
        attach_link(ctx, &self.dbx, membership_id, policy_id, &meta).await
    }

    /// Detaches a single policy from the membership (idempotent).
    pub async fn detach_policy(
        &self,
        ctx: &StoreCtx,
        membership_id: &DbId,
        policy_id: &DbId,
    ) -> StoreResult<()> {
        let meta = self.policy_many_to_many_meta();
        detach_link(ctx, &self.dbx, membership_id, policy_id, &meta).await
    }

    /// Metadata for the `membership_policy` many-to-many join (mirrors the
    /// `membership_role` metadata exposed by `ManyToManyStore`).
    fn policy_many_to_many_meta(&self) -> ManyToManyQueryMeta<MembershipIden> {
        ManyToManyQueryMeta {
            single_table: MembershipIden::Table,
            many_table: MembershipIden::Policy,
            join_table: MembershipIden::MembershipPolicy,
            single_pk: MembershipIden::Id,
            many_pk: MembershipIden::PolicyPk,
            many_fk: MembershipIden::PolicyId,
            join_fk: MembershipIden::MembershipId,
            agg_alias: MembershipIden::Policies,
            has_audit: true,
        }
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, MembershipStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for MembershipStore<D> {
    type Iden = MembershipIden;
    type Row = MembershipRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for MembershipStore<D> {
    type FilterStoreParams = MembershipFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: MembershipIden::Table,
            pk: MembershipIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for MembershipStore<D> {
    type CreateStoreParams = MembershipForCreate;
    type UpdateStoreParams = MembershipForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: MembershipIden::Table,
            pk: MembershipIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ManyToManyStore for MembershipStore<D> {
    type ManyToManyRow = MembershipWithRoles;

    type FilterStoreParams = MembershipFilter;

    fn many_to_many_meta(&self) -> ManyToManyQueryMeta<Self::Iden> {
        ManyToManyQueryMeta {
            single_table: MembershipIden::Table,
            many_table: MembershipIden::Role,
            join_table: MembershipIden::MembershipRole,
            single_pk: MembershipIden::Id,
            many_pk: MembershipIden::RolePk,
            many_fk: MembershipIden::RoleId,
            join_fk: MembershipIden::MembershipId,
            agg_alias: MembershipIden::Roles,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for MembershipStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: MembershipIden::Table,
            col: MembershipIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: MembershipIden::Table,
            col: MembershipIden::Meta,
            has_audit: true,
        }
    }
}

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------

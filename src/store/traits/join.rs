use crate::store::ctx::StoreCtx;
use crate::store::error::StoreResult;
use crate::store::queries::join::{
    attach_link, detach_link, get_many_to_many, get_many_to_many_opt, get_one_to_many,
    get_one_to_many_opt, list_many_to_many, list_one_to_many, set_many_to_many_links,
};
use crate::store::traits::meta::{HasId, ManyToManyStore, OneToManyStore, ReadStore, StoreRow};
use modql::filter::ListOptions;

// --- One to Many ---

/// Trait for getting a single parent record with its children aggregated.
pub trait GetOneToMany: OneToManyStore {
    /// Fetches a single record by ID, with its related records aggregated.
    async fn get_one_to_many(
        &self,
        ctx: &StoreCtx,
        id: &<Self::OneToManyRow as HasId>::Id,
    ) -> StoreResult<Self::OneToManyRow> {
        let dbx = self.dbx();
        let meta = self.one_to_many_meta();
        get_one_to_many(ctx, &dbx, id, &meta).await
    }

    /// Fetches a single record by ID, returning `Ok(None)` if not found.
    async fn get_one_to_many_opt(
        &self,
        ctx: &StoreCtx,
        id: &<Self::OneToManyRow as HasId>::Id,
    ) -> StoreResult<Option<Self::OneToManyRow>> {
        let dbx = self.dbx();
        let meta = self.one_to_many_meta();
        get_one_to_many_opt(ctx, &dbx, id, &meta).await
    }
}

/// Trait for listing parent records with their children aggregated.
pub trait ListOneToMany: OneToManyStore {
    /// Fetches a list of records, each with their related records aggregated.
    async fn list_one_to_many(
        &self,
        ctx: &StoreCtx,
        filter: Option<Self::FilterStoreParams>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<Self::OneToManyRow>> {
        let dbx = self.dbx();
        let meta = self.one_to_many_meta();
        list_one_to_many(ctx, &dbx, filter, opts, &meta).await
    }
}

// --- Many to Many ---

/// Trait for getting a single record with its many-to-many relations aggregated.
pub trait GetManyToMany: ManyToManyStore {
    /// Fetches a single record by ID, with its related records aggregated.
    async fn get_many_to_many(
        &self,
        ctx: &StoreCtx,
        id: &<Self::ManyToManyRow as HasId>::Id,
    ) -> StoreResult<Self::ManyToManyRow> {
        let dbx = self.dbx();
        let meta = self.many_to_many_meta();
        get_many_to_many(ctx, &dbx, id, &meta).await
    }

    /// Fetches a single record by ID, returning `Ok(None)` if not found.
    async fn get_many_to_many_opt(
        &self,
        ctx: &StoreCtx,
        id: &<Self::ManyToManyRow as HasId>::Id,
    ) -> StoreResult<Option<Self::ManyToManyRow>> {
        let dbx = self.dbx();
        let meta = self.many_to_many_meta();
        get_many_to_many_opt(ctx, &dbx, id, &meta).await
    }
}

/// Trait for listing records with their many-to-many relations aggregated.
pub trait ListManyToMany: ManyToManyStore {
    /// Fetches a list of records, each with their related records aggregated.
    async fn list_many_to_many(
        &self,
        ctx: &StoreCtx,
        filter: Option<Self::FilterStoreParams>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<Self::ManyToManyRow>> {
        let dbx = self.dbx();
        let meta = self.many_to_many_meta();
        list_many_to_many(ctx, &dbx, filter, opts, &meta).await
    }
}

/// Trait for modifying the links in a many-to-many join table.
pub trait LinkManyToMany: ManyToManyStore {
    /// Sets the definitive list of links for a record.
    async fn set_many_to_many_links(
        &self,
        ctx: &StoreCtx,
        self_id: &<Self::ManyToManyRow as HasId>::Id,
        other_ids: Vec<<Self::ManyToManyRow as HasId>::Id>,
    ) -> StoreResult<()> {
        let dbx = self.dbx();
        let meta = self.many_to_many_meta();
        set_many_to_many_links(ctx, &dbx, self_id, other_ids, &meta).await
    }

    /// Creates a single link between a record and another record.
    async fn attach_link(
        &self,
        ctx: &StoreCtx,
        self_id: &<Self::Row as HasId>::Id,
        other_id: &<Self::Row as HasId>::Id,
    ) -> StoreResult<()> {
        let dbx = self.dbx();
        let meta = self.many_to_many_meta();
        attach_link(ctx, &dbx, self_id, other_id, &meta).await
    }

    /// Removes a single link between a record and another record.
    async fn detach_link(
        &self,
        ctx: &StoreCtx,
        self_id: &<Self::Row as HasId>::Id,
        other_id: &<Self::Row as HasId>::Id,
    ) -> StoreResult<()> {
        let dbx = self.dbx();
        let meta = self.many_to_many_meta();
        detach_link(ctx, &dbx, self_id, other_id, &meta).await
    }
}

use modql::filter::ListOptions;
use serde_json::Value as JsonValue;

use crate::store::ctx::StoreCtx;
use crate::store::error::StoreResult;
use crate::store::queries::contains::filter_by_value_contains;
use crate::store::queries::count::count_contains;
use crate::store::queries::meta::ContainsFilter;
use crate::store::traits::meta::{ContainsFilterStore, Store};

/// Trait for filtering records where a PostgreSQL array or JSONB column contains specific values,
/// utilizing the database's containment operator (`@>`).
pub trait FilterByContains: ContainsFilterStore {
    /// Finds all records where the designated array column (e.g., `tags`)
    /// **fully contains** all of the specified `tags`.
    ///
    /// This uses the `ContainsFilter::Array` variant.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `tags`: A vector of strings (the subset) that the column must contain.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing a vector of matching rows.
    async fn filter_by_tags_contain(
        &self,
        ctx: &StoreCtx,
        tags: Vec<String>,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.contains_tags_meta();
        let value = ContainsFilter::Array(tags);
        filter_by_value_contains(ctx, &dbx, value, opts, &meta).await
    }

    /// Returns a count of all records where the designated array column (e.g., `tags`)
    /// **fully contains** all of the specified `tags`.
    ///
    /// This delegates to the generic `count_contains` query function, using the
    /// PostgreSQL containment operator (`@>`) with the `ContainsFilter::Array` variant.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `tags`: A vector of strings (the subset) that the column must contain.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing the total count as an `i64`.
    async fn count_by_tags_contain(&self, ctx: &StoreCtx, tags: Vec<String>) -> StoreResult<i64> {
        let dbx = self.dbx();
        let meta = self.contains_tags_meta();
        let value = ContainsFilter::Array(tags);
        count_contains(ctx, &dbx, value, &meta).await
    }

    /// Finds all records where the designated JSONB column **fully contains** the
    /// key/value pairs specified in the `json` filter object.
    ///
    /// This uses the `ContainsFilter::Json` variant.
    ///
    /// # Arguments
    ///
    /// * `ctx`: The store context.
    /// * `json`: A JSON object (`serde_json::Value`) representing the required subset of keys and values.
    ///
    /// # Returns
    ///
    /// A `StoreResult` containing a vector of matching rows.
    async fn filter_by_json_contains(
        &self,
        ctx: &StoreCtx,
        json: JsonValue,
        opts: Option<ListOptions>,
    ) -> StoreResult<Vec<Self::Row>> {
        let dbx = self.dbx();
        let meta = self.contains_json_meta();
        let value = ContainsFilter::Json(json);
        filter_by_value_contains(ctx, &dbx, value, opts, &meta).await
    }
}

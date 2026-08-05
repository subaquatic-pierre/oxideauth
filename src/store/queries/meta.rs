use sea_query::{Iden, IntoIden, IntoTableRef, TableRef};
use serde_json::Value;

use crate::store::traits::meta::TableIden;

/// Metadata for read-only operations like `list`, `get`, `first`, `count`.
pub struct ReadQueryMeta<I: TableIden> {
    /// The primary table identifier for the query.
    pub table: I,
    /// The identifier for the primary key column.
    pub pk: I,
    /// Flag indicating if the table includes audit fields (e.g., `created_at`, `updated_at`).
    pub has_audit: bool,
}

/// Metadata for mutating operations like `create`, `update`, `delete`.
pub struct MutateQueryMeta<I: TableIden> {
    /// The primary table identifier being mutated.
    pub table: I,
    /// The primary key column identifier.
    pub pk: I,
    /// Flag indicating if audit fields (e.g., `updated_at`, `updated_by`) need to be handled during mutation.
    pub has_audit: bool,
}

/// Metadata for fetching a single "parent" entity and embedding its "many" associated children (collection).
pub struct OneToManyQueryMeta<I: TableIden> {
    /// The parent table (the "one" side of the relationship).
    pub single_table: I,
    /// The child table (the "many" side of the relationship).
    pub many_table: I,
    /// The primary key column identifier of the parent table.
    pub single_pk: I,
    /// The primary key column identifier of the child table.
    pub many_pk: I,
    /// The Foreign Key column in the **child** table that references the parent (e.g., `child.parent_id`).
    pub many_fk: I,
    /// The column alias used for the `jsonb_agg` result (i.e., the field name for the embedded collection).
    pub agg_alias: I,
    /// Flag indicating if the parent table has audit fields.
    pub has_audit: bool,
}

/// Metadata for fetching a single "parent" entity and embedding its associated collection
/// through an intermediary **join table** (Many-to-Many relationship).
pub struct ManyToManyQueryMeta<I: TableIden> {
    /// The parent table (the starting entity, e.g., 'roles').
    pub single_table: I,
    /// The final collection table (the entities being aggregated, e.g., 'permissions').
    pub many_table: I,
    /// The intermediary table linking `single_table` and `many_table` (e.g., 'role_permissions').
    pub join_table: I,
    /// Foreign Key in the **join table** pointing to the parent (`single_table.single_pk`, e.g., 'role_id').
    pub join_fk: I,
    /// The primary key column identifier of the parent table.
    pub single_pk: I,
    /// The primary key column identifier of the final collection table.
    pub many_pk: I,
    /// Foreign Key in the **join table** pointing to the collection table (`many_table.many_pk`, e.g., 'permission_id').
    pub many_fk: I,
    /// The column alias for the `jsonb_agg` result (i.e., the field name for the embedded collection in the resulting struct).
    pub agg_alias: I,
    /// Audit flag for the parent table.
    pub has_audit: bool,
}

/// Minimal metadata used to count child records in a one-to-many relationship (e.g., in a limit check).
pub struct CountManyQueryMeta<I: TableIden> {
    /// The child table being counted.
    pub table: I,
    /// The Foreign Key column in the child table used for filtering (e.g., `user_id` in a `credential` table).
    pub fk: I,
}
/// Defines the value type for PostgreSQL containment queries (`@>`) used on array or JSONB columns.
#[derive(Clone)]
pub enum ContainsFilter {
    /// Represents an array of strings to check for containment within a PostgreSQL array column.
    Array(Vec<String>),
    /// Represents a JSON structure (`serde_json::Value`) to check for containment within a JSONB column.
    Json(Value),
}

/// Metadata for queries filtering by a containment condition (e.g., checking if a column contains specific tags).
pub struct ContainsFilterQueryMeta<I: TableIden> {
    /// The table to be queried.
    pub table: I,
    /// The column identifier that holds the array or JSONB data to be checked.
    pub col: I,
    /// Audit flag for the parent table.
    pub has_audit: bool,
}

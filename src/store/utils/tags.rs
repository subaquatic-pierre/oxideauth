use sea_query::{Alias, BinOper, Expr, ExprTrait, Iden, SelectStatement};

/// Applies a PostgreSQL array-containment (`@>`) condition on the `tags` column.
///
/// This adds `WHERE {table}.tags @> $1::text[]` to the query so that only rows
/// whose `tags` array contains **all** of the given tags are returned.
///
/// An empty `tags` vector is a no-op: an empty containment filter would otherwise
/// match every row (the empty array is a subset of every array), so it is treated
/// as "no tag filter" to preserve existing semantics.
pub fn apply_tags_to_query(query: &mut SelectStatement, table: impl Iden, tags: Vec<String>) {
    if tags.is_empty() {
        return;
    }

    let expr = Expr::col((table, Alias::new("tags"))).binary(
        BinOper::Custom("@>"),
        Expr::val(tags).cast_as(Alias::new("text[]")),
    );
    query.and_where(expr);
}

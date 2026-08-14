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

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::{Alias, Asterisk, PostgresQueryBuilder, Query, SelectStatement};

    #[test]
    fn test_apply_tags_to_query_empty_is_noop() {
        // -- Setup
        let mut query: SelectStatement = Query::select();
        query.from(Alias::new("account")).column(Asterisk);

        // -- Execute
        apply_tags_to_query(&mut query, Alias::new("account"), vec![]);

        // -- Assert
        let sql = query.to_string(PostgresQueryBuilder);
        assert!(
            !sql.contains("WHERE"),
            "empty tags must not add a WHERE clause: {sql}"
        );
    }

    #[test]
    fn test_apply_tags_to_query_non_empty_adds_contains() {
        // -- Setup
        let mut query: SelectStatement = Query::select();
        query.from(Alias::new("account")).column(Asterisk);

        // -- Execute
        apply_tags_to_query(
            &mut query,
            Alias::new("account"),
            vec!["admin".to_string(), "prod".to_string()],
        );

        // -- Assert
        let sql = query.to_string(PostgresQueryBuilder);
        assert!(sql.contains("@>"), "expected array containment in SQL: {sql}");
        assert!(sql.contains("WHERE"), "expected a WHERE clause: {sql}");
    }
}

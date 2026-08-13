use sea_query::{Alias, BinOper, Expr, ExprTrait, Iden, SelectStatement};

pub fn apply_tags_to_query(query: &mut SelectStatement, table: impl Iden, tags: Vec<String>) {
    // --- Tags containment (@>) ---
    let expr = Expr::col((table, Alias::new("tags"))).binary(
        BinOper::Custom("@>"),
        Expr::val(tags).cast_as(Alias::new("text[]")),
    );
    query.and_where(expr);
}

pub mod extraction;
pub mod glob_matcher;
pub mod hardcoded;
pub mod key_objects;
pub mod schema;
pub mod translation_calls;
pub mod value_analyzer;
pub mod value_source;

use swc_ecma_ast::Expr;

/// Unwrap parentheses and TypeScript type assertions.
/// Handles: `(expr)`, `expr as T`, `expr as const`, `expr satisfies T`
pub fn unwrap_paren(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(paren) => unwrap_paren(&paren.expr),
        Expr::TsAs(ts_as) => unwrap_paren(&ts_as.expr),
        Expr::TsConstAssertion(ts_const) => unwrap_paren(&ts_const.expr),
        Expr::TsSatisfies(ts_sat) => unwrap_paren(&ts_sat.expr),
        _ => expr,
    }
}

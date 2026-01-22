pub mod extraction;
pub mod glob_matcher;
pub mod hardcoded;
pub mod key_objects;
pub mod registry_collector;
pub mod schema;
pub mod translation_calls;
pub mod value_analyzer;
pub mod value_source;

use swc_ecma_ast::{CallExpr, Expr, Lit};

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

/// Translation hook function names from next-intl.
pub const TRANSLATION_HOOKS: &[&str] = &["useTranslations", "getTranslations"];

/// Check if a function name is a translation hook (useTranslations or getTranslations).
pub fn is_translation_hook(name: &str) -> bool {
    TRANSLATION_HOOKS.contains(&name)
}

/// Extract namespace from translation hook call.
///
/// Example: `useTranslations("MyNamespace")` -> `Some("MyNamespace")`
pub fn extract_namespace_from_call(call: &CallExpr) -> Option<String> {
    call.args.first().and_then(|arg| {
        if let Expr::Lit(Lit::Str(s)) = &*arg.expr {
            s.value.as_str().map(|s| s.to_string())
        } else {
            None
        }
    })
}

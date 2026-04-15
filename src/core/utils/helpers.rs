//! Helper functions for AST analysis.

use swc_ecma_ast::{CallExpr, Expr, Lit, ObjectPat, ObjectPatProp, Pat};

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
pub const NEXT_INTL_HOOKS: &[&str] = &["useTranslations", "getTranslations"];

/// Translation hook function names from react-i18next.
pub const REACT_I18NEXT_HOOKS: &[&str] = &["useTranslation"];

/// Check if a function name is a translation hook (any supported framework).
pub fn is_translation_hook(name: &str) -> bool {
    NEXT_INTL_HOOKS.contains(&name) || REACT_I18NEXT_HOOKS.contains(&name)
}

/// Check if the hook returns an object that needs destructuring (react-i18next).
/// e.g., `const { t } = useTranslation("ns")`
pub fn is_destructuring_hook(name: &str) -> bool {
    REACT_I18NEXT_HOOKS.contains(&name)
}

/// Extract the `t` binding name from a destructuring pattern for react-i18next hooks.
///
/// Handles:
/// - `const { t } = useTranslation("ns")` → Some("t")
/// - `const { t, i18n } = useTranslation("ns")` → Some("t") (only `t`)
/// - `const { t: translate } = useTranslation("ns")` → None (renamed away from `t`)
///
/// Only the `t` binding is tracked because `useTranslation()` returns an object
/// with multiple fields (`t`, `i18n`, `ready`), and only `t` is the translation
/// function. This differs from next-intl where `useTranslations()` returns the
/// translation function directly.
pub fn extract_t_from_destructuring(pat: &Pat) -> Option<String> {
    let Pat::Object(obj_pat) = pat else {
        return None;
    };
    extract_t_from_object_pat(obj_pat)
}

/// Inner helper: extract `t` from ObjectPat props.
fn extract_t_from_object_pat(obj_pat: &ObjectPat) -> Option<String> {
    for prop in &obj_pat.props {
        let name = match prop {
            ObjectPatProp::Assign(assign) => Some(assign.key.sym.to_string()),
            ObjectPatProp::KeyValue(kv) => {
                if let Pat::Ident(ident) = &*kv.value {
                    Some(ident.id.sym.to_string())
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(ref n) = name {
            if n == "t" {
                return Some(n.clone());
            }
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_translation_hook_next_intl() {
        assert!(is_translation_hook("useTranslations"));
        assert!(is_translation_hook("getTranslations"));
    }

    #[test]
    fn test_is_translation_hook_react_i18next() {
        assert!(is_translation_hook("useTranslation"));
    }

    #[test]
    fn test_is_translation_hook_unknown() {
        assert!(!is_translation_hook("useFormatter"));
        assert!(!is_translation_hook("t"));
    }

    #[test]
    fn test_is_destructuring_hook_react_i18next() {
        assert!(is_destructuring_hook("useTranslation"));
    }

    #[test]
    fn test_is_destructuring_hook_next_intl_returns_false() {
        assert!(!is_destructuring_hook("useTranslations"));
        assert!(!is_destructuring_hook("getTranslations"));
    }
}

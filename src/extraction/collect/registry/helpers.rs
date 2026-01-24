//! Helper functions for AST node extraction.
//!
//! Pure functions with no state dependencies, extracted from the original
//! RegistryCollector for better organization.

use std::collections::HashMap;

use swc_ecma_ast::{ArrayLit, CallExpr, Expr, Lit, ObjectLit, PropName, TsEntityName, TsType};

/// Check if a TypeScript type is `TFunction`.
pub fn is_tfunction_type(ts_type: &TsType) -> bool {
    match ts_type {
        TsType::TsTypeRef(type_ref) => {
            if let TsEntityName::Ident(ident) = &type_ref.type_name {
                ident.sym.as_str() == "TFunction"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Extract string argument from a call expression.
pub fn extract_string_arg(call: &CallExpr) -> Option<String> {
    call.args.first().and_then(|arg| match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => s.value.as_str().map(|s| s.to_string()),
        Expr::Tpl(tpl) if tpl.exprs.is_empty() => tpl
            .quasis
            .first()
            .and_then(|q| q.cooked.as_ref())
            .and_then(|s| s.as_str().map(|s| s.to_string())),
        _ => None,
    })
}

/// Check if a call expression passes a specific parameter as first argument.
pub fn is_schema_call_with_t(call: &CallExpr, t_param_name: &str) -> bool {
    call.args.first().is_some_and(|arg| {
        if let Expr::Ident(ident) = &*arg.expr {
            ident.sym.as_str() == t_param_name
        } else {
            false
        }
    })
}

/// Unwrap TypeScript-specific expression wrappers to get the inner expression.
pub fn unwrap_ts_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::TsAs(ts_as) => unwrap_ts_expr(&ts_as.expr),
        Expr::TsConstAssertion(ts_const) => unwrap_ts_expr(&ts_const.expr),
        Expr::TsSatisfies(ts_sat) => unwrap_ts_expr(&ts_sat.expr),
        Expr::Paren(paren) => unwrap_ts_expr(&paren.expr),
        _ => expr,
    }
}

/// Extract string values from an object literal.
/// Returns None if object has spread or no string values.
pub fn extract_string_value(obj: &ObjectLit) -> Option<Vec<String>> {
    use swc_ecma_ast::{Prop, PropOrSpread};

    let mut values = Vec::new();
    let mut has_spread = false;

    for prop in &obj.props {
        match prop {
            PropOrSpread::Spread(_) => {
                has_spread = true;
            }
            PropOrSpread::Prop(prop) => {
                if let Prop::KeyValue(kv) = &**prop
                    && matches!(&kv.key, PropName::Ident(_) | PropName::Str(_))
                    && let Expr::Lit(Lit::Str(s)) = &*kv.value
                    && let Some(v) = s.value.as_str()
                {
                    values.push(v.to_string());
                }
            }
        }
    }

    if has_spread || values.is_empty() {
        None
    } else {
        Some(values)
    }
}

/// Extract string values from an array literal.
/// Returns None if array has spread or non-string elements.
pub fn extract_string_array(arr: &ArrayLit) -> Option<Vec<String>> {
    use swc_ecma_ast::ExprOrSpread;

    let mut values = Vec::new();

    for elem in &arr.elems {
        match elem {
            Some(ExprOrSpread {
                spread: Some(_), ..
            }) => {
                return None;
            }
            Some(ExprOrSpread { expr, spread: None }) => {
                let inner = unwrap_ts_expr(expr);
                match inner {
                    Expr::Lit(Lit::Str(s)) => {
                        if let Some(v) = s.value.as_str() {
                            values.push(v.to_string());
                        } else {
                            return None;
                        }
                    }
                    _ => {
                        return None;
                    }
                }
            }
            None => {}
        }
    }

    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

/// Extract property values from an array of objects.
/// Returns a map of property names to their values across all objects.
pub fn extract_array_properties(arr: &ArrayLit) -> Option<HashMap<String, Vec<String>>> {
    use swc_ecma_ast::{ExprOrSpread, Prop, PropOrSpread};

    let mut property_values: HashMap<String, Vec<String>> = HashMap::new();
    let mut has_spread = false;

    for elem in &arr.elems {
        match elem {
            Some(ExprOrSpread {
                spread: Some(_), ..
            }) => {
                has_spread = true;
            }
            Some(ExprOrSpread { expr, spread: None }) => {
                if let Expr::Object(obj) = &**expr {
                    for prop in &obj.props {
                        if let PropOrSpread::Prop(prop) = prop
                            && let Prop::KeyValue(kv) = &**prop
                            && let Some(prop_name) = extract_prop_name(&kv.key)
                            && let Expr::Lit(Lit::Str(s)) = &*kv.value
                            && let Some(v) = s.value.as_str()
                        {
                            property_values
                                .entry(prop_name)
                                .or_default()
                                .push(v.to_string());
                        }
                    }
                }
            }
            None => {}
        }
    }

    if has_spread || property_values.is_empty() {
        None
    } else {
        Some(property_values)
    }
}

/// Extract property name from a PropName node.
pub fn extract_prop_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(ident) => Some(ident.sym.to_string()),
        PropName::Str(s) => s.value.as_str().map(|s| s.to_string()),
        _ => None,
    }
}

/// Extract the full name from a JSX member expression (e.g., "Foo.Bar.Baz").
pub fn extract_jsx_member_name(member: &swc_ecma_ast::JSXMemberExpr) -> String {
    let object_name = match &member.obj {
        swc_ecma_ast::JSXObject::Ident(ident) => ident.sym.to_string(),
        swc_ecma_ast::JSXObject::JSXMemberExpr(nested) => extract_jsx_member_name(nested),
    };
    format!("{}.{}", object_name, member.prop.sym)
}

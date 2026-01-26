//! Schema function collection state.
//!
//! Handles the detection and collection of schema functions - arrow functions
//! that take a translation function parameter and contain translation keys.

use swc_ecma_ast::{ArrowExpr, CallExpr, Callee, Expr, Pat, VarDeclarator};

use crate::analysis::schema::{SchemaFunction, SchemaFunctionContext};

use crate::analysis::collect::registry::helpers::{
    extract_string_arg, is_schema_call_with_t, is_tfunction_type,
};

/// Internal state for schema function collection.
///
/// This struct manages the temporary state needed during schema function
/// detection, separate from the output storage.
pub struct SchemaInternalState {
    /// Currently being analyzed schema function context.
    current_context: Option<SchemaFunctionContext>,
    /// Depth of nested functions that shadow the schema t_param_name.
    shadow_depth: usize,
}

impl SchemaInternalState {
    pub fn new() -> Self {
        Self {
            current_context: None,
            shadow_depth: 0,
        }
    }

    /// Check if a variable declarator defines a schema function.
    ///
    /// A schema function is an arrow function whose first parameter:
    /// - Starts with 't', OR
    /// - Has type annotation `TFunction`
    ///
    /// Returns the arrow expression to visit if this is a schema function.
    pub fn check_schema_function<'a>(&mut self, decl: &'a VarDeclarator) -> Option<&'a ArrowExpr> {
        let func_name = match &decl.name {
            Pat::Ident(ident) => ident.id.sym.to_string(),
            _ => return None,
        };

        let arrow = match &decl.init {
            Some(expr) => match &**expr {
                Expr::Arrow(arrow) => arrow,
                _ => return None,
            },
            _ => return None,
        };

        let t_param_name = Self::extract_t_param(arrow)?;

        self.current_context = Some(SchemaFunctionContext {
            name: func_name,
            t_param_name,
            keys: Vec::new(),
            nested_calls: Vec::new(),
        });

        Some(arrow)
    }

    /// Finalize schema function after visiting its body.
    ///
    /// Takes the collected context and pushes it to the output if it has
    /// any keys or nested calls.
    pub fn finalize_schema_function(&mut self, file_path: &str, output: &mut Vec<SchemaFunction>) {
        if let Some(ctx) = self.current_context.take()
            && (!ctx.keys.is_empty() || !ctx.nested_calls.is_empty())
        {
            output.push(SchemaFunction {
                name: ctx.name,
                file_path: file_path.to_string(),
                keys: ctx.keys,
                nested_calls: ctx.nested_calls,
            });
        }
    }

    /// Check a call expression for translation keys or nested schema calls.
    ///
    /// Only collects if:
    /// - We're inside a schema function (current_context is Some)
    /// - Not inside a scope that shadows the t parameter
    pub fn check_call_expr(&mut self, node: &CallExpr) {
        // Only collect keys if not inside a shadowed scope
        if self.shadow_depth == 0
            && let Some(ctx) = &mut self.current_context
            && let Callee::Expr(expr) = &node.callee
            && let Expr::Ident(ident) = &**expr
        {
            let fn_name = ident.sym.as_str();
            if fn_name == ctx.t_param_name {
                // Direct t() call - extract the key
                if let Some(key) = extract_string_arg(node) {
                    ctx.keys.push(key);
                }
            } else if is_schema_call_with_t(node, &ctx.t_param_name) {
                // Nested schema call - track for dependency resolution
                ctx.nested_calls.push(fn_name.to_string());
            }
        }
    }

    /// Check if function parameters shadow the t parameter.
    ///
    /// Returns true if any parameter has the same name as the current
    /// schema function's t parameter.
    pub fn check_shadow<'a>(&self, params: impl Iterator<Item = &'a Pat>) -> bool {
        self.current_context.as_ref().is_some_and(|ctx| {
            params.into_iter().any(|param| {
                if let Pat::Ident(ident) = param {
                    ident.id.sym.as_str() == ctx.t_param_name
                } else {
                    false
                }
            })
        })
    }

    /// Enter a scope that shadows the t parameter.
    pub fn enter_shadow(&mut self) {
        self.shadow_depth += 1;
    }

    /// Exit a scope that shadows the t parameter.
    pub fn exit_shadow(&mut self) {
        self.shadow_depth -= 1;
    }

    /// Extract the t parameter name from an arrow function.
    fn extract_t_param(arrow: &ArrowExpr) -> Option<String> {
        arrow.params.first().and_then(|param| {
            if let Pat::Ident(ident) = param {
                let name = ident.id.sym.to_string();
                let has_tfunction_type = ident
                    .type_ann
                    .as_ref()
                    .map(|ann| is_tfunction_type(&ann.type_ann))
                    .unwrap_or(false);
                if name.starts_with('t') || has_tfunction_type {
                    Some(name)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
}

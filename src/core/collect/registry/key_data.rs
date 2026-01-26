//! Key data collection state.
//!
//! Handles the collection of key objects, arrays, imports, translation props,
//! and translation function calls.

use std::collections::HashMap;
use std::path::Path;

use swc_ecma_ast::{
    CallExpr, Callee, DefaultDecl, Expr, ImportSpecifier, JSXAttr, JSXAttrName, JSXAttrOrSpread,
    JSXAttrValue, JSXElement, JSXElementName, JSXExpr, ModuleExportName, VarDecl,
};

use crate::core::utils::{extract_namespace_from_call, is_translation_hook};

use crate::core::collect::registry::helpers::{
    extract_array_properties, extract_jsx_member_name, extract_string_array, extract_string_value,
    unwrap_ts_expr,
};
use crate::core::collect::types::{
    FileImports, ImportInfo, KeyArray, KeyObject, StringArray, TranslationBindingValue,
    TranslationFnCall, TranslationProp, resolve_import_path,
};

/// Internal state for key data collection.
///
/// Manages the binding scope stack for tracking translation function bindings.
/// The output collections are stored separately in RegistryCollector.
pub struct KeyDataInternalState {
    /// Stack of translation function bindings scoped by function/arrow.
    bindings_stack: Vec<HashMap<String, TranslationBindingValue>>,
}

impl KeyDataInternalState {
    pub fn new() -> Self {
        Self {
            bindings_stack: vec![HashMap::new()],
        }
    }

    // ============================================================
    // Scope management
    // ============================================================

    /// Enter a new binding scope (function/arrow).
    pub fn enter_scope(&mut self) {
        self.bindings_stack.push(HashMap::new());
    }

    /// Exit the current binding scope.
    pub fn exit_scope(&mut self) {
        self.bindings_stack.pop();
    }

    /// Shadow bindings with the given names in the current scope.
    pub fn shadow_bindings(&mut self, names: impl Iterator<Item = String>) {
        if let Some(scope) = self.bindings_stack.last_mut() {
            for name in names {
                scope.insert(name, TranslationBindingValue::Shadowed);
            }
        }
    }

    /// Get the translation binding for a variable name.
    /// Returns Some(Some(namespace)) for translation bindings,
    /// Some(None) for translation bindings without namespace,
    /// None if not a translation binding or shadowed.
    pub fn get_translation_binding(&self, name: &str) -> Option<Option<String>> {
        for scope in self.bindings_stack.iter().rev() {
            if let Some(value) = scope.get(name) {
                return match value {
                    TranslationBindingValue::Translation(namespace) => Some(namespace.clone()),
                    TranslationBindingValue::Shadowed => None,
                };
            }
        }
        None
    }

    // ============================================================
    // Variable declaration handling
    // ============================================================

    /// Check a variable declaration for key objects/arrays and translation bindings.
    #[allow(clippy::too_many_arguments)]
    pub fn check_var_decl(
        &mut self,
        node: &VarDecl,
        is_exported: bool,
        is_module_level: bool,
        file_path: &str,
        objects: &mut Vec<KeyObject>,
        arrays: &mut Vec<KeyArray>,
        string_arrays: &mut Vec<StringArray>,
    ) {
        for decl in &node.decls {
            let name = match &decl.name {
                swc_ecma_ast::Pat::Ident(ident) => ident.id.sym.to_string(),
                _ => continue,
            };

            let Some(init) = &decl.init else { continue };

            // Track translation function bindings
            self.check_translation_binding(&name, init);

            let inner_expr = unwrap_ts_expr(init);

            match inner_expr {
                Expr::Object(obj) => {
                    if let Some(candidate_keys) = extract_string_value(obj) {
                        objects.push(KeyObject {
                            name,
                            file_path: file_path.to_string(),
                            is_exported,
                            is_module_level,
                            candidate_keys,
                        });
                    }
                }
                Expr::Array(arr) => {
                    // Try as string array first
                    if let Some(values) = extract_string_array(arr) {
                        string_arrays.push(StringArray {
                            name,
                            file_path: file_path.to_string(),
                            is_exported,
                            is_module_level,
                            values,
                        });
                    }
                    // Otherwise try as object array
                    else if let Some(property_values) = extract_array_properties(arr) {
                        arrays.push(KeyArray {
                            name,
                            file_path: file_path.to_string(),
                            is_exported,
                            is_module_level,
                            property_values,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// Check and track a translation binding.
    fn check_translation_binding(&mut self, var_name: &str, init: &Expr) {
        let call_expr = match init {
            Expr::Call(call) => Some(call),
            Expr::Await(await_expr) => match &*await_expr.arg {
                Expr::Call(call) => Some(call),
                _ => None,
            },
            _ => None,
        };

        if let Some(call) = call_expr
            && let Callee::Expr(expr) = &call.callee
            && let Expr::Ident(ident) = &**expr
        {
            let fn_name = ident.sym.as_str();
            if is_translation_hook(fn_name) {
                let namespace = extract_namespace_from_call(call);
                self.insert_translation_binding(var_name.to_string(), namespace);
            }
        }
    }

    /// Insert a translation binding into the current scope.
    fn insert_translation_binding(&mut self, name: String, namespace: Option<String>) {
        if let Some(scope) = self.bindings_stack.last_mut() {
            scope.insert(name, TranslationBindingValue::Translation(namespace));
        }
    }

    // ============================================================
    // Import handling
    // ============================================================

    /// Collect import declarations.
    pub fn collect_import(&mut self, node: &swc_ecma_ast::ImportDecl, imports: &mut FileImports) {
        if let Some(module_path) = node.src.value.as_str() {
            for specifier in &node.specifiers {
                match specifier {
                    ImportSpecifier::Named(named) => {
                        let local_name = named.local.sym.to_string();
                        let imported_name = named
                            .imported
                            .as_ref()
                            .map(|i| match i {
                                ModuleExportName::Ident(ident) => ident.sym.to_string(),
                                ModuleExportName::Str(s) => s.value.to_string_lossy().to_string(),
                            })
                            .unwrap_or_else(|| local_name.clone());

                        imports.push(ImportInfo {
                            local_name,
                            imported_name,
                            module_path: module_path.to_string(),
                        });
                    }
                    ImportSpecifier::Default(default) => {
                        imports.push(ImportInfo {
                            local_name: default.local.sym.to_string(),
                            imported_name: "default".to_string(),
                            module_path: module_path.to_string(),
                        });
                    }
                    ImportSpecifier::Namespace(ns) => {
                        imports.push(ImportInfo {
                            local_name: ns.local.sym.to_string(),
                            imported_name: "*".to_string(),
                            module_path: module_path.to_string(),
                        });
                    }
                }
            }
        }
    }

    // ============================================================
    // Default export handling
    // ============================================================

    /// Check export default declaration.
    pub fn check_default_export_decl(
        &mut self,
        node: &swc_ecma_ast::ExportDefaultDecl,
        default_export_name: &mut Option<String>,
    ) {
        match &node.decl {
            DefaultDecl::Fn(fn_expr) => {
                *default_export_name = fn_expr
                    .ident
                    .as_ref()
                    .map(|ident| ident.sym.to_string())
                    .or_else(|| Some("default".to_string()));
            }
            DefaultDecl::Class(class_expr) => {
                *default_export_name = class_expr
                    .ident
                    .as_ref()
                    .map(|ident| ident.sym.to_string())
                    .or_else(|| Some("default".to_string()));
            }
            _ => {}
        }
    }

    /// Check export default expression.
    pub fn check_default_export_expr(
        &mut self,
        node: &swc_ecma_ast::ExportDefaultExpr,
        default_export_name: &mut Option<String>,
    ) {
        match &*node.expr {
            Expr::Ident(ident) => {
                *default_export_name = Some(ident.sym.to_string());
            }
            Expr::Arrow(_) | Expr::Fn(_) => {
                *default_export_name = Some("default".to_string());
            }
            _ => {}
        }
    }

    // ============================================================
    // Call expression handling
    // ============================================================

    /// Check a call expression for translation function arguments.
    pub fn check_call_expr(
        &self,
        node: &CallExpr,
        file_path: &str,
        imports: &FileImports,
        translation_fn_calls: &mut Vec<TranslationFnCall>,
    ) {
        if let Callee::Expr(callee_expr) = &node.callee
            && let Expr::Ident(fn_ident) = &**callee_expr
        {
            let local_fn_name = fn_ident.sym.to_string();

            if !is_translation_hook(&local_fn_name) {
                let (fn_file_path, fn_name) =
                    Self::resolve_fn_definition(&local_fn_name, file_path, imports);

                for (idx, arg) in node.args.iter().enumerate() {
                    if let Expr::Ident(arg_ident) = &*arg.expr {
                        let var_name = arg_ident.sym.to_string();

                        if let Some(namespace) = self.get_translation_binding(&var_name) {
                            Self::add_or_update_translation_fn_call(
                                &fn_file_path,
                                &fn_name,
                                idx,
                                namespace,
                                translation_fn_calls,
                            );
                        }
                    }
                }
            }
        }
    }

    // ============================================================
    // JSX element handling
    // ============================================================

    /// Check a JSX element for translation props.
    pub fn check_jsx_element(
        &self,
        node: &JSXElement,
        translation_props: &mut Vec<TranslationProp>,
    ) {
        let component_name = match &node.opening.name {
            JSXElementName::Ident(ident) => {
                let name = ident.sym.to_string();
                if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    Some(name)
                } else {
                    None
                }
            }
            JSXElementName::JSXMemberExpr(member) => Some(extract_jsx_member_name(member)),
            JSXElementName::JSXNamespacedName(_) => None,
        };

        if let Some(comp_name) = component_name {
            for attr in &node.opening.attrs {
                if let JSXAttrOrSpread::JSXAttr(JSXAttr {
                    name: JSXAttrName::Ident(prop_ident),
                    value: Some(JSXAttrValue::JSXExprContainer(container)),
                    ..
                }) = attr
                {
                    let prop_name = prop_ident.sym.to_string();

                    if let JSXExpr::Expr(expr) = &container.expr
                        && let Expr::Ident(value_ident) = &**expr
                    {
                        let var_name = value_ident.sym.to_string();

                        if let Some(namespace) = self.get_translation_binding(&var_name) {
                            Self::add_or_update_translation_prop(
                                &comp_name,
                                &prop_name,
                                namespace,
                                translation_props,
                            );
                        }
                    }
                }
            }
        }
    }

    // ============================================================
    // Helper methods
    // ============================================================

    /// Resolve a function name to its definition location.
    fn resolve_fn_definition(
        local_fn_name: &str,
        file_path: &str,
        imports: &FileImports,
    ) -> (String, String) {
        if let Some(import) = imports.iter().find(|i| i.local_name == local_fn_name) {
            let resolved_path = resolve_import_path(Path::new(file_path), &import.module_path)
                .unwrap_or_else(|| file_path.to_string());
            (resolved_path, import.imported_name.clone())
        } else {
            (file_path.to_string(), local_fn_name.to_string())
        }
    }

    /// Add or update a translation prop.
    fn add_or_update_translation_prop(
        component_name: &str,
        prop_name: &str,
        namespace: Option<String>,
        translation_props: &mut Vec<TranslationProp>,
    ) {
        if let Some(existing) = translation_props
            .iter_mut()
            .find(|p| p.component_name == component_name && p.prop_name == prop_name)
        {
            if !existing.namespaces.contains(&namespace) {
                existing.namespaces.push(namespace);
            }
        } else {
            translation_props.push(TranslationProp {
                component_name: component_name.to_string(),
                prop_name: prop_name.to_string(),
                namespaces: vec![namespace],
            });
        }
    }

    /// Add or update a translation function call.
    fn add_or_update_translation_fn_call(
        fn_file_path: &str,
        fn_name: &str,
        arg_index: usize,
        namespace: Option<String>,
        translation_fn_calls: &mut Vec<TranslationFnCall>,
    ) {
        if let Some(existing) = translation_fn_calls.iter_mut().find(|c| {
            c.fn_file_path == fn_file_path && c.fn_name == fn_name && c.arg_index == arg_index
        }) {
            if !existing.namespaces.contains(&namespace) {
                existing.namespaces.push(namespace);
            }
        } else {
            translation_fn_calls.push(TranslationFnCall {
                fn_file_path: fn_file_path.to_string(),
                fn_name: fn_name.to_string(),
                arg_index,
                namespaces: vec![namespace],
            });
        }
    }
}

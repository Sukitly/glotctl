//! Registry collection - Schema functions, key objects, and cross-file dependencies.
//!
//! This module implements the RegistryCollector, which performs Phase 1 collection
//! of cross-file dependency data in a single AST pass per file.
//!
//! ## Module Structure
//!
//! - `helpers`: Pure functions for AST node extraction
//! - `schema`: Schema function detection and collection
//! - `key_data`: Key objects, arrays, imports, and translation bindings
//!
//! ## Single-Pass Optimization
//!
//! The collector merges multiple sub-collectors (schema functions, key objects, imports,
//! translation bindings) into a single visitor to minimize AST traversals.

mod helpers;
mod key_data;
mod schema;

use swc_ecma_ast::{ArrowExpr, Decl, Function, VarDecl};
use swc_ecma_visit::{Visit, VisitWith};

use key_data::KeyDataInternalState;
use schema::SchemaInternalState;

use crate::core::schema::SchemaFunction;

use crate::core::collect::types::{
    FileImports, KeyArray, KeyObject, StringArray, TranslationFnCall, TranslationProp,
    extract_binding_names,
};

/// Combined collector that gathers both schema functions and key objects
/// in a single AST traversal.
///
/// # Usage
///
/// ```ignore
/// let mut collector = RegistryCollector::new("path/to/file.tsx");
/// module.visit_with(&mut collector);
///
/// // Access collected data
/// for func in collector.schema_functions { ... }
/// for obj in collector.objects { ... }
/// ```
pub struct RegistryCollector {
    // === Shared fields ===
    /// Path to the file being analyzed.
    pub file_path: String,
    /// Tracks nesting depth: 0 = module level, >0 = inside function/arrow.
    scope_depth: usize,

    // === Output fields (pub for external access) ===
    /// Collected schema functions.
    pub schema_functions: Vec<SchemaFunction>,
    /// Collected key objects (object literals with string values).
    pub objects: Vec<KeyObject>,
    /// Collected key arrays (arrays of objects with string properties).
    pub arrays: Vec<KeyArray>,
    /// Collected string arrays.
    pub string_arrays: Vec<StringArray>,
    /// Collected import declarations.
    pub imports: FileImports,
    /// Collected translation props (JSX components receiving translation functions).
    pub translation_props: Vec<TranslationProp>,
    /// Collected translation function calls.
    pub translation_fn_calls: Vec<TranslationFnCall>,
    /// Name of the default export, if any.
    pub default_export_name: Option<String>,

    // === Internal state (private) ===
    /// Schema collection state.
    schema_state: SchemaInternalState,
    /// Key data collection state.
    key_data_state: KeyDataInternalState,
}

impl RegistryCollector {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            scope_depth: 0,
            schema_functions: Vec::new(),
            objects: Vec::new(),
            arrays: Vec::new(),
            string_arrays: Vec::new(),
            imports: Vec::new(),
            translation_props: Vec::new(),
            translation_fn_calls: Vec::new(),
            default_export_name: None,
            schema_state: SchemaInternalState::new(),
            key_data_state: KeyDataInternalState::new(),
        }
    }

    /// Check variable declaration for schema functions.
    /// Returns the arrow body to visit if a schema function is found.
    fn check_schema_function<'a>(
        &mut self,
        decl: &'a swc_ecma_ast::VarDeclarator,
    ) -> Option<&'a ArrowExpr> {
        self.schema_state.check_schema_function(decl)
    }

    /// Finalize schema function after visiting its body.
    fn finalize_schema_function(&mut self) {
        self.schema_state
            .finalize_schema_function(&self.file_path, &mut self.schema_functions);
    }

    /// Check variable declarations for key objects/arrays.
    fn check_var_decl(&mut self, node: &VarDecl, is_exported: bool, is_module_level: bool) {
        self.key_data_state.check_var_decl(
            node,
            is_exported,
            is_module_level,
            &self.file_path,
            &mut self.objects,
            &mut self.arrays,
            &mut self.string_arrays,
        );
    }
}

impl Visit for RegistryCollector {
    fn visit_export_decl(&mut self, node: &swc_ecma_ast::ExportDecl) {
        if let Decl::Var(var_decl) = &node.decl {
            // Schema logic: check for schema functions
            for decl in &var_decl.decls {
                if let Some(arrow) = self.check_schema_function(decl) {
                    arrow.body.visit_with(self);
                    self.finalize_schema_function();
                }
            }

            // Key data logic: collect exported variables
            let is_module_level = self.scope_depth == 0;
            self.check_var_decl(var_decl, true, is_module_level);
        } else {
            node.visit_children_with(self);
        }
    }

    fn visit_export_default_decl(&mut self, node: &swc_ecma_ast::ExportDefaultDecl) {
        self.key_data_state
            .check_default_export_decl(node, &mut self.default_export_name);
        node.visit_children_with(self);
    }

    fn visit_export_default_expr(&mut self, node: &swc_ecma_ast::ExportDefaultExpr) {
        self.key_data_state
            .check_default_export_expr(node, &mut self.default_export_name);
        node.visit_children_with(self);
    }

    fn visit_var_decl(&mut self, node: &VarDecl) {
        let is_module_level = self.scope_depth == 0;
        self.check_var_decl(node, false, is_module_level);
        node.visit_children_with(self);
    }

    fn visit_import_decl(&mut self, node: &swc_ecma_ast::ImportDecl) {
        self.key_data_state.collect_import(node, &mut self.imports);
    }

    fn visit_function(&mut self, node: &Function) {
        // Enter scope
        self.scope_depth += 1;
        self.key_data_state.enter_scope();

        // Shadow parameter bindings
        self.key_data_state.shadow_bindings(
            node.params
                .iter()
                .flat_map(|p| extract_binding_names(&p.pat)),
        );

        // Check for schema t parameter shadowing
        let shadows_t = self
            .schema_state
            .check_shadow(node.params.iter().map(|p| &p.pat));
        if shadows_t {
            self.schema_state.enter_shadow();
        }

        // Visit children
        node.visit_children_with(self);

        // Exit scope
        if shadows_t {
            self.schema_state.exit_shadow();
        }
        self.key_data_state.exit_scope();
        self.scope_depth -= 1;
    }

    fn visit_arrow_expr(&mut self, node: &ArrowExpr) {
        // Enter scope
        self.scope_depth += 1;
        self.key_data_state.enter_scope();

        // Shadow parameter bindings
        self.key_data_state
            .shadow_bindings(node.params.iter().flat_map(extract_binding_names));

        // Check for schema t parameter shadowing
        let shadows_t = self.schema_state.check_shadow(node.params.iter());
        if shadows_t {
            self.schema_state.enter_shadow();
        }

        // Visit children
        node.visit_children_with(self);

        // Exit scope
        if shadows_t {
            self.schema_state.exit_shadow();
        }
        self.key_data_state.exit_scope();
        self.scope_depth -= 1;
    }

    fn visit_call_expr(&mut self, node: &swc_ecma_ast::CallExpr) {
        // Schema logic: check for t() calls
        self.schema_state.check_call_expr(node);

        // Key data logic: check for translation functions passed as arguments
        self.key_data_state.check_call_expr(
            node,
            &self.file_path,
            &self.imports,
            &mut self.translation_fn_calls,
        );

        node.visit_children_with(self);
    }

    fn visit_jsx_element(&mut self, node: &swc_ecma_ast::JSXElement) {
        // Key data logic: check for translation props
        self.key_data_state
            .check_jsx_element(node, &mut self.translation_props);

        node.visit_children_with(self);
    }
}

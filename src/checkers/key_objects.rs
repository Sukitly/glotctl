use std::{collections::HashMap, path::Path};

use swc_ecma_ast::{
    ArrayLit, ArrowExpr, Callee, Decl, Expr, ExprOrSpread, Function, ImportSpecifier, JSXAttr,
    JSXAttrName, JSXAttrOrSpread, JSXAttrValue, JSXElement, JSXElementName, JSXExpr, Lit,
    ModuleExportName, ObjectLit, Pat, Prop, PropName, PropOrSpread, VarDecl,
};
use swc_ecma_visit::{Visit, VisitWith};

/// Represents an object literal containing translation key candidates
#[derive(Debug, Clone)]
pub struct KeyObject {
    pub name: String,
    pub file_path: String,
    pub is_exported: bool,
    /// Whether defined at module level (scope_depth == 0)
    pub is_module_level: bool,
    pub candidate_keys: Vec<String>,
}

/// Registry of all objects that may contain translation key mappings
/// Key format: "file_path:object_name"
pub type KeyObjectRegistry = HashMap<String, KeyObject>;

/// Tracks import statements for cross-file resolution
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub local_name: String,
    pub imported_name: String,
    pub module_path: String,
}

/// Collected imports for a single file
pub type FileImports = Vec<ImportInfo>;

/// Represents an array of objects containing translation key candidates
/// Used for patterns like: const items = [{ key: "a" }, { key: "b" }]
#[derive(Debug, Clone)]
pub struct KeyArray {
    pub name: String,
    pub file_path: String,
    pub is_exported: bool,
    /// Whether defined at module level (scope_depth == 0)
    pub is_module_level: bool,
    /// Maps property names to all values found in array elements
    /// e.g., { "titleKey": ["novelManagement", "characterDevelopment"] }
    pub property_values: HashMap<String, Vec<String>>,
}

/// Registry of all arrays that may contain translation key mappings
/// Key format: "file_path.array_name"
pub type KeyArrayRegistry = HashMap<String, KeyArray>;

/// Represents a string array containing translation key candidates
/// Used for patterns like: const KEYS = ["save", "characters"] as const;
#[derive(Debug, Clone)]
pub struct StringArray {
    pub name: String,
    pub file_path: String,
    pub is_exported: bool,
    /// Whether defined at module level (scope_depth == 0)
    pub is_module_level: bool,
    /// All string values in the array
    pub values: Vec<String>,
}

/// Registry of all string arrays that may contain translation keys
/// Key format: "file_path.array_name"
pub type StringArrayRegistry = HashMap<String, StringArray>;

/// Represents a translation function passed as a JSX prop.
/// Used to track: `<ComponentName t={translationVar} />`
#[derive(Debug, Clone)]
pub struct TranslationProp {
    /// Component name (e.g., "AdultLandingPage")
    pub component_name: String,
    /// Prop name (e.g., "t")
    pub prop_name: String,
    /// List of possible namespaces from different call sites
    /// None means no namespace (e.g., `useTranslations()` without argument)
    pub namespaces: Vec<Option<String>>,
}

/// Registry of translation functions passed as JSX props.
/// Key format: "ComponentName.propName"
pub type TranslationPropRegistry = HashMap<String, TranslationProp>;

/// Create registry key for TranslationPropRegistry
pub fn make_translation_prop_key(component_name: &str, prop_name: &str) -> String {
    format!("{}.{}", component_name, prop_name)
}

/// Represents a translation function passed as a regular function call argument.
/// Used to track: `someFunc(t)` where `t` is a translation function.
///
/// This enables tracking translation keys in utility/factory functions that
/// receive translation functions as parameters, not just React components.
#[derive(Debug, Clone)]
pub struct TranslationFnCall {
    /// File path where the function is defined
    pub fn_file_path: String,
    /// Function name (e.g., "usageTypeLabels")
    pub fn_name: String,
    /// Argument index (0-based) where the translation function is passed
    pub arg_index: usize,
    /// List of possible namespaces from different call sites
    /// None means no namespace (e.g., `useTranslations()` without argument)
    pub namespaces: Vec<Option<String>>,
}

/// Registry of translation functions passed as regular function call arguments.
/// Key format: "file_path.fn_name.arg_index"
pub type TranslationFnCallRegistry = HashMap<String, TranslationFnCall>;

/// Create registry key for TranslationFnCallRegistry
pub fn make_translation_fn_call_key(fn_file_path: &str, fn_name: &str, arg_index: usize) -> String {
    format!("{}.{}.{}", fn_file_path, fn_name, arg_index)
}

/// Collects object literals, arrays, imports, and translation props from TypeScript/TSX files
pub struct KeyObjectCollector {
    pub file_path: String,
    pub objects: Vec<KeyObject>,
    pub arrays: Vec<KeyArray>,
    pub string_arrays: Vec<StringArray>,
    pub imports: FileImports,
    /// Translation functions passed as JSX props
    pub translation_props: Vec<TranslationProp>,
    /// Translation functions passed as regular function call arguments
    pub translation_fn_calls: Vec<TranslationFnCall>,
    /// Tracks nesting depth: 0 = module level, >0 = inside function/arrow
    scope_depth: usize,
    /// Stack of translation function bindings scoped by function/arrow.
    /// Used to detect when a translation function is passed as a prop or function argument.
    translation_bindings_stack: Vec<HashMap<String, Option<String>>>,
}

const TRANSLATION_HOOKS: &[&str] = &["useTranslations", "getTranslations"];

fn is_translation_hook(name: &str) -> bool {
    TRANSLATION_HOOKS.contains(&name)
}

/// Extract namespace from translation hook call: useTranslations("MyNamespace") -> Some("MyNamespace")
fn extract_namespace_from_call(call: &swc_ecma_ast::CallExpr) -> Option<String> {
    call.args.first().and_then(|arg| {
        if let Expr::Lit(Lit::Str(s)) = &*arg.expr {
            s.value.as_str().map(|s| s.to_string())
        } else {
            None
        }
    })
}

impl KeyObjectCollector {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            objects: Vec::new(),
            arrays: Vec::new(),
            string_arrays: Vec::new(),
            imports: Vec::new(),
            translation_props: Vec::new(),
            translation_fn_calls: Vec::new(),
            scope_depth: 0,
            translation_bindings_stack: vec![HashMap::new()],
        }
    }

    fn enter_binding_scope(&mut self) {
        self.translation_bindings_stack.push(HashMap::new());
    }

    fn exit_binding_scope(&mut self) {
        self.translation_bindings_stack.pop();
    }

    fn insert_translation_binding(&mut self, name: String, namespace: Option<String>) {
        if let Some(scope) = self.translation_bindings_stack.last_mut() {
            scope.insert(name, namespace);
        }
    }

    fn get_translation_binding(&self, name: &str) -> Option<Option<String>> {
        for scope in self.translation_bindings_stack.iter().rev() {
            if let Some(namespace) = scope.get(name) {
                return Some(namespace.clone());
            }
        }
        None
    }

    /// Extract all string values from an object literal
    fn extract_string_value(&self, obj: &ObjectLit) -> Option<Vec<String>> {
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

    /// Extract property name from PropName
    fn extract_prop_name(key: &PropName) -> Option<String> {
        match key {
            PropName::Ident(ident) => Some(ident.sym.to_string()),
            PropName::Str(s) => s.value.as_str().map(|s| s.to_string()),
            _ => None,
        }
    }

    /// Extract all string values from a pure string array
    /// e.g., ["save", "characters", "chapters"] -> Some(["save", "characters", "chapters"])
    fn extract_string_array(&self, arr: &ArrayLit) -> Option<Vec<String>> {
        let mut values = Vec::new();

        for elem in &arr.elems {
            match elem {
                Some(ExprOrSpread {
                    spread: Some(_), ..
                }) => {
                    // Has spread, cannot statically determine all values
                    return None;
                }
                Some(ExprOrSpread { expr, spread: None }) => {
                    let inner = Self::unwrap_ts_expr(expr);
                    match inner {
                        Expr::Lit(Lit::Str(s)) => {
                            if let Some(v) = s.value.as_str() {
                                values.push(v.to_string());
                            } else {
                                return None; // Non-UTF8 string
                            }
                        }
                        _ => {
                            // Non-string element, this is not a pure string array
                            return None;
                        }
                    }
                }
                None => {
                    // Empty slot (sparse array), skip
                }
            }
        }

        if values.is_empty() {
            None
        } else {
            Some(values)
        }
    }

    /// Extract all string property values from an array of objects
    /// Returns a map of property name -> list of values
    fn extract_array_properties(&self, arr: &ArrayLit) -> Option<HashMap<String, Vec<String>>> {
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
                                && let Some(prop_name) = Self::extract_prop_name(&kv.key)
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

    /// Unwrap TypeScript type assertions to get the inner expression
    fn unwrap_ts_expr(expr: &Expr) -> &Expr {
        match expr {
            Expr::TsAs(ts_as) => Self::unwrap_ts_expr(&ts_as.expr),
            Expr::TsConstAssertion(ts_const) => Self::unwrap_ts_expr(&ts_const.expr),
            Expr::TsSatisfies(ts_sat) => Self::unwrap_ts_expr(&ts_sat.expr),
            Expr::Paren(paren) => Self::unwrap_ts_expr(&paren.expr),
            _ => expr,
        }
    }

    fn check_var_decl(&mut self, node: &VarDecl, is_exported: bool, is_module_level: bool) {
        for decl in &node.decls {
            let name = match &decl.name {
                Pat::Ident(ident) => ident.id.sym.to_string(),
                _ => continue,
            };

            let Some(init) = &decl.init else { continue };

            // Track translation function bindings: const t = useTranslations("Ns")
            self.check_translation_binding(&name, init);

            let inner_expr = Self::unwrap_ts_expr(init);

            match inner_expr {
                Expr::Object(obj) => {
                    if let Some(candidate_keys) = self.extract_string_value(obj) {
                        self.objects.push(KeyObject {
                            name,
                            file_path: self.file_path.clone(),
                            is_exported,
                            is_module_level,
                            candidate_keys,
                        });
                    }
                }
                Expr::Array(arr) => {
                    // Try as string array first (e.g., ["save", "load"])
                    if let Some(values) = self.extract_string_array(arr) {
                        self.string_arrays.push(StringArray {
                            name,
                            file_path: self.file_path.clone(),
                            is_exported,
                            is_module_level,
                            values,
                        });
                    }
                    // Otherwise try as object array (e.g., [{ key: "a" }, { key: "b" }])
                    else if let Some(property_values) = self.extract_array_properties(arr) {
                        self.arrays.push(KeyArray {
                            name,
                            file_path: self.file_path.clone(),
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

    /// Check if this is a translation function binding and track it.
    /// Handles: const t = useTranslations("Ns") or const t = await getTranslations("Ns")
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
}

impl Visit for KeyObjectCollector {
    fn visit_function(&mut self, node: &Function) {
        self.scope_depth += 1;
        self.enter_binding_scope();
        node.visit_children_with(self);
        self.exit_binding_scope();
        self.scope_depth -= 1;
    }

    fn visit_arrow_expr(&mut self, node: &ArrowExpr) {
        self.scope_depth += 1;
        self.enter_binding_scope();
        node.visit_children_with(self);
        self.exit_binding_scope();
        self.scope_depth -= 1;
    }

    fn visit_export_decl(&mut self, node: &swc_ecma_ast::ExportDecl) {
        if let Decl::Var(var_decl) = &node.decl {
            // Collect all exports, marking whether they're at module level
            let is_module_level = self.scope_depth == 0;
            self.check_var_decl(var_decl, true, is_module_level);
        } else {
            // Only traverse children for non-Var declarations (e.g., export function)
            node.visit_children_with(self);
        }
    }

    fn visit_var_decl(&mut self, node: &VarDecl) {
        // Collect all declarations, marking whether they're at module level
        // Exported ones are handled by visit_export_decl
        let is_module_level = self.scope_depth == 0;
        self.check_var_decl(node, false, is_module_level);
        node.visit_children_with(self);
    }

    fn visit_import_decl(&mut self, node: &swc_ecma_ast::ImportDecl) {
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

                        self.imports.push(ImportInfo {
                            local_name,
                            imported_name,
                            module_path: module_path.to_string(),
                        });
                    }
                    ImportSpecifier::Default(default) => {
                        self.imports.push(ImportInfo {
                            local_name: default.local.sym.to_string(),
                            imported_name: "default".to_string(),
                            module_path: module_path.to_string(),
                        });
                    }
                    ImportSpecifier::Namespace(ns) => {
                        self.imports.push(ImportInfo {
                            local_name: ns.local.sym.to_string(),
                            imported_name: "*".to_string(),
                            module_path: module_path.to_string(),
                        });
                    }
                }
            }
        }
    }

    /// Visit JSX elements to detect translation functions passed as props.
    /// e.g., <AdultLandingPage t={t} /> where t is a translation function
    fn visit_jsx_element(&mut self, node: &JSXElement) {
        // Extract component name
        let component_name = match &node.opening.name {
            JSXElementName::Ident(ident) => {
                let name = ident.sym.to_string();
                // Only process PascalCase components (user-defined components)
                // Skip lowercase elements like <div>, <span>
                if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    Some(name)
                } else {
                    None
                }
            }
            JSXElementName::JSXMemberExpr(member) => {
                // Handle Foo.Bar -> "Foo.Bar"
                Some(Self::extract_jsx_member_name(member))
            }
            JSXElementName::JSXNamespacedName(_) => {
                // Namespaced names like xml:space are rare, skip for now
                None
            }
        };

        // Process props if we have a valid component name
        if let Some(comp_name) = component_name {
            for attr in &node.opening.attrs {
                if let JSXAttrOrSpread::JSXAttr(JSXAttr {
                    name: JSXAttrName::Ident(prop_ident),
                    value: Some(JSXAttrValue::JSXExprContainer(container)),
                    ..
                }) = attr
                {
                    let prop_name = prop_ident.sym.to_string();

                    // Check if the value is a known translation function
                    if let JSXExpr::Expr(expr) = &container.expr
                        && let Expr::Ident(value_ident) = &**expr
                    {
                        let var_name = value_ident.sym.to_string();

                        // If this variable is a tracked translation function, record it
                        if let Some(namespace) = self.get_translation_binding(&var_name) {
                            self.add_or_update_translation_prop(
                                &comp_name,
                                &prop_name,
                                namespace.clone(),
                            );
                        }
                    }
                }
            }
        }

        // Continue visiting children
        node.visit_children_with(self);
    }

    /// Visit function calls to detect translation functions passed as arguments.
    /// e.g., `usageTypeLabels(t)` where `t` is a translation function.
    fn visit_call_expr(&mut self, node: &swc_ecma_ast::CallExpr) {
        // Only process direct function calls (not method calls)
        if let Callee::Expr(callee_expr) = &node.callee
            && let Expr::Ident(fn_ident) = &**callee_expr
        {
            let fn_name = fn_ident.sym.to_string();

            // Skip translation hooks themselves - we don't want to track useTranslations(namespace)
            if is_translation_hook(&fn_name) {
                node.visit_children_with(self);
                return;
            }

            // Resolve where this function is defined
            let fn_file_path = self.resolve_fn_definition_path(&fn_name);

            // Check each argument to see if it's a translation function
            for (idx, arg) in node.args.iter().enumerate() {
                if let Expr::Ident(arg_ident) = &*arg.expr {
                    let var_name = arg_ident.sym.to_string();

                    // If this variable is a tracked translation function, record it
                    if let Some(namespace) = self.get_translation_binding(&var_name) {
                        self.add_or_update_translation_fn_call(
                            &fn_file_path,
                            &fn_name,
                            idx,
                            namespace.clone(),
                        );
                    }
                }
            }
        }

        // Continue visiting children
        node.visit_children_with(self);
    }
}

impl KeyObjectCollector {
    /// Extract full member expression name: Foo.Bar.Baz
    fn extract_jsx_member_name(member: &swc_ecma_ast::JSXMemberExpr) -> String {
        let object_name = match &member.obj {
            swc_ecma_ast::JSXObject::Ident(ident) => ident.sym.to_string(),
            swc_ecma_ast::JSXObject::JSXMemberExpr(nested) => Self::extract_jsx_member_name(nested),
        };
        format!("{}.{}", object_name, member.prop.sym)
    }

    /// Add or update a translation prop entry.
    /// If the same component.prop already exists, merge the namespace.
    fn add_or_update_translation_prop(
        &mut self,
        component_name: &str,
        prop_name: &str,
        namespace: Option<String>,
    ) {
        // Check if we already have an entry for this component.prop
        if let Some(existing) = self
            .translation_props
            .iter_mut()
            .find(|p| p.component_name == component_name && p.prop_name == prop_name)
        {
            // Only add if this namespace isn't already tracked
            if !existing.namespaces.contains(&namespace) {
                existing.namespaces.push(namespace);
            }
        } else {
            // Create new entry
            self.translation_props.push(TranslationProp {
                component_name: component_name.to_string(),
                prop_name: prop_name.to_string(),
                namespaces: vec![namespace],
            });
        }
    }

    /// Resolve where a function is defined based on its name.
    ///
    /// If the function is imported, returns the resolved file path of the import.
    /// Otherwise, returns the current file path (function defined locally).
    fn resolve_fn_definition_path(&self, fn_name: &str) -> String {
        // Check if the function is imported
        if let Some(import) = self.imports.iter().find(|i| i.local_name == fn_name) {
            // Try to resolve the import path to an actual file
            resolve_import_path(Path::new(&self.file_path), &import.module_path)
                .unwrap_or_else(|| self.file_path.clone())
        } else {
            // Function is defined locally
            self.file_path.clone()
        }
    }

    /// Add or update a translation function call entry.
    /// If the same fn_file_path.fn_name.arg_index already exists, merge the namespace.
    fn add_or_update_translation_fn_call(
        &mut self,
        fn_file_path: &str,
        fn_name: &str,
        arg_index: usize,
        namespace: Option<String>,
    ) {
        // Check if we already have an entry for this function.arg_index
        if let Some(existing) = self.translation_fn_calls.iter_mut().find(|c| {
            c.fn_file_path == fn_file_path && c.fn_name == fn_name && c.arg_index == arg_index
        }) {
            // Only add if this namespace isn't already tracked
            if !existing.namespaces.contains(&namespace) {
                existing.namespaces.push(namespace);
            }
        } else {
            // Create new entry
            self.translation_fn_calls.push(TranslationFnCall {
                fn_file_path: fn_file_path.to_string(),
                fn_name: fn_name.to_string(),
                arg_index,
                namespaces: vec![namespace],
            });
        }
    }
}

/// Resolves a TypeScript import path to an actual file path
pub fn resolve_import_path(current_file: &Path, import_path: &str) -> Option<String> {
    // Only handle relative imports
    if !import_path.starts_with('.') {
        return None;
    }

    let base_dir = current_file.parent()?;
    let resolved = base_dir.join(import_path);

    for ext in &["ts", "tsx", "js", "jsx"] {
        let with_ext = resolved.with_extension(ext);
        if with_ext.exists() {
            return Some(with_ext.to_string_lossy().to_string());
        }
    }

    for ext in &["ts", "tsx", "js", "jsx"] {
        let index_path = resolved.join(format!("index.{}", ext));
        if index_path.exists() {
            return Some(index_path.to_string_lossy().to_string());
        }
    }

    // Fallback: return speculative ".ts" path for registry lookup.
    //
    // Why: When checking file A that imports from file B, file B might not
    // have been scanned yet. By returning a speculative path, the registry
    // key will still match when file B is eventually processed.
    //
    // This is safe because if the import is invalid, TypeScript compilation
    // would have already failed before this tool runs.
    Some(resolved.with_extension("ts").to_string_lossy().to_string())
}

pub fn make_registry_key(file_path: &str, name: &str) -> String {
    format!("{}.{}", file_path, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::FileName;
    use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};
    use swc_ecma_visit::VisitWith;

    fn parse_and_collect(code: &str) -> KeyObjectCollector {
        let source_map = swc_common::SourceMap::default();
        let source_file =
            source_map.new_source_file(FileName::Real("test.ts".into()).into(), code.to_string());

        let syntax = Syntax::Typescript(TsSyntax {
            tsx: true,
            ..Default::default()
        });
        let mut parser = Parser::new(syntax, StringInput::from(&*source_file), None);
        let module = parser.parse_module().unwrap();

        let mut collector = KeyObjectCollector::new("test.ts");
        module.visit_with(&mut collector);
        collector
    }

    #[test]
    fn test_collect_exported_object() {
        let code = r#"
            export const toolKeys = {
                createNovel: "createNovel",
                updateNovel: "updateNovel",
            };
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.objects.len(), 1);
        assert!(collector.objects[0].is_exported);
        assert_eq!(collector.objects[0].name, "toolKeys");
        assert_eq!(
            collector.objects[0].candidate_keys,
            vec!["createNovel", "updateNovel"]
        );
    }

    #[test]
    fn test_collect_non_exported_object() {
        let code = r#"
            const toolKeys = {
                key1: "value1",
                key2: "value2",
            };
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.objects.len(), 1);
        assert!(!collector.objects[0].is_exported);
        assert_eq!(collector.objects[0].name, "toolKeys");
    }

    #[test]
    fn test_collect_object_with_as_const() {
        let code = r#"
            const keys = {
                a: "alpha",
                b: "beta",
            } as const;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.objects.len(), 1);
        assert_eq!(collector.objects[0].candidate_keys, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_collect_object_with_satisfies() {
        let code = r#"
            const keys = {
                a: "alpha",
            } satisfies Record<string, string>;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.objects.len(), 1);
        assert_eq!(collector.objects[0].candidate_keys, vec!["alpha"]);
    }

    #[test]
    fn test_skip_object_with_spread() {
        let code = r#"
            const base = { a: "a" };
            const extended = { ...base, b: "b" };
        "#;
        let collector = parse_and_collect(code);

        // First object is collected, second is skipped due to spread
        assert_eq!(collector.objects.len(), 1);
        assert_eq!(collector.objects[0].name, "base");
    }

    #[test]
    fn test_collect_imports() {
        let code = r#"
            import { toolKeys } from "./tool-keys";
            import { otherKeys } from "../constants";
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.imports.len(), 2);
        assert_eq!(collector.imports[0].local_name, "toolKeys");
        assert_eq!(collector.imports[0].imported_name, "toolKeys");
        assert_eq!(collector.imports[0].module_path, "./tool-keys");
    }

    #[test]
    fn test_collect_renamed_imports() {
        let code = r#"
            import { toolKeys as keys } from "./tool-keys";
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.imports.len(), 1);
        assert_eq!(collector.imports[0].local_name, "keys");
        assert_eq!(collector.imports[0].imported_name, "toolKeys");
    }

    #[test]
    fn test_collect_default_import() {
        let code = r#"
            import toolKeys from "./tool-keys";
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.imports.len(), 1);
        assert_eq!(collector.imports[0].local_name, "toolKeys");
        assert_eq!(collector.imports[0].imported_name, "default");
    }

    #[test]
    fn test_skip_non_string_values() {
        let code = r#"
            const keys = {
                a: "string_value",
                b: 123,
                c: null,
                d: true,
            };
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.objects.len(), 1);
        // Only string value is collected
        assert_eq!(collector.objects[0].candidate_keys, vec!["string_value"]);
    }

    #[test]
    fn test_collect_array_of_objects() {
        let code = r#"
            const capabilities = [
                { titleKey: "novelManagement", descriptionKey: "novelDesc" },
                { titleKey: "characterDevelopment", descriptionKey: "charDesc" },
            ];
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.arrays.len(), 1);
        assert_eq!(collector.arrays[0].name, "capabilities");
        assert!(!collector.arrays[0].is_exported);

        let property_values = &collector.arrays[0].property_values;
        assert_eq!(
            property_values.get("titleKey"),
            Some(&vec![
                "novelManagement".to_string(),
                "characterDevelopment".to_string()
            ])
        );
        assert_eq!(
            property_values.get("descriptionKey"),
            Some(&vec!["novelDesc".to_string(), "charDesc".to_string()])
        );
    }

    #[test]
    fn test_collect_exported_array() {
        let code = r#"
            export const items = [
                { key: "item1" },
                { key: "item2" },
            ];
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.arrays.len(), 1);
        assert!(collector.arrays[0].is_exported);
        assert_eq!(collector.arrays[0].name, "items");
    }

    #[test]
    fn test_skip_array_with_spread() {
        let code = r#"
            const base = [{ key: "a" }];
            const extended = [...base, { key: "b" }];
        "#;
        let collector = parse_and_collect(code);

        // First array is collected, second is skipped due to spread
        assert_eq!(collector.arrays.len(), 1);
        assert_eq!(collector.arrays[0].name, "base");
    }

    #[test]
    fn test_array_with_type_annotation() {
        let code = r#"
            interface Item {
                key: string;
            }
            const items: Item[] = [
                { key: "value1" },
                { key: "value2" },
            ];
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.arrays.len(), 1);
        assert_eq!(
            collector.arrays[0].property_values.get("key"),
            Some(&vec!["value1".to_string(), "value2".to_string()])
        );
    }

    #[test]
    fn test_array_with_as_const() {
        let code = r#"
            const items = [
                { key: "value1" },
                { key: "value2" },
            ] as const;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.arrays.len(), 1);
        assert_eq!(
            collector.arrays[0].property_values.get("key"),
            Some(&vec!["value1".to_string(), "value2".to_string()])
        );
    }

    // ============================================================
    // StringArray test cases
    // ============================================================

    #[test]
    fn test_collect_string_array() {
        let code = r#"
            const FEATURE_KEYS = ["save", "characters", "chapters", "ai"];
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.string_arrays.len(), 1);
        assert_eq!(collector.string_arrays[0].name, "FEATURE_KEYS");
        assert!(!collector.string_arrays[0].is_exported);
        assert_eq!(
            collector.string_arrays[0].values,
            vec!["save", "characters", "chapters", "ai"]
        );
    }

    #[test]
    fn test_collect_string_array_with_as_const() {
        let code = r#"
            const KEYS = ["a", "b", "c"] as const;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.string_arrays.len(), 1);
        assert_eq!(collector.string_arrays[0].values, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_collect_exported_string_array() {
        let code = r#"
            export const SUPPORTED_GENDERS = ["male", "female", "other"] as const;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.string_arrays.len(), 1);
        assert!(collector.string_arrays[0].is_exported);
        assert_eq!(collector.string_arrays[0].name, "SUPPORTED_GENDERS");
        assert_eq!(
            collector.string_arrays[0].values,
            vec!["male", "female", "other"]
        );
    }

    #[test]
    fn test_skip_string_array_with_spread() {
        let code = r#"
            const base = ["a", "b"];
            const extended = [...base, "c"];
        "#;
        let collector = parse_and_collect(code);

        // First array is collected, second is skipped due to spread
        assert_eq!(collector.string_arrays.len(), 1);
        assert_eq!(collector.string_arrays[0].name, "base");
    }

    #[test]
    fn test_skip_mixed_array() {
        let code = r#"
            const mixed = ["string", 123, true];
        "#;
        let collector = parse_and_collect(code);

        // Mixed array is not collected as string array
        assert!(collector.string_arrays.is_empty());
        // Also not collected as object array
        assert!(collector.arrays.is_empty());
    }

    #[test]
    fn test_string_array_preferred_over_object_array() {
        // If an array contains only strings, it should be collected as StringArray,
        // not as KeyArray (object array)
        let code = r#"
            const KEYS = ["key1", "key2"];
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.string_arrays.len(), 1);
        assert!(collector.arrays.is_empty()); // Not collected as object array
    }

    // ============================================================
    // Scope depth tests - collect all declarations, mark module-level
    // ============================================================

    #[test]
    fn test_nested_string_array_in_function() {
        let code = r#"
            const GLOBAL = ["a", "b"];
            function foo() {
                const LOCAL = ["c", "d"];  // Collected but not module-level
            }
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.string_arrays.len(), 2);

        let global = collector
            .string_arrays
            .iter()
            .find(|a| a.name == "GLOBAL")
            .unwrap();
        assert!(global.is_module_level);

        let local = collector
            .string_arrays
            .iter()
            .find(|a| a.name == "LOCAL")
            .unwrap();
        assert!(!local.is_module_level);
    }

    #[test]
    fn test_nested_object_in_arrow_function() {
        let code = r#"
            const GLOBAL_OBJ = { key: "value" };
            const fn = () => {
                const LOCAL_OBJ = { nested: "ignored" };  // Collected but not module-level
            };
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.objects.len(), 2);

        let global = collector
            .objects
            .iter()
            .find(|o| o.name == "GLOBAL_OBJ")
            .unwrap();
        assert!(global.is_module_level);

        let local = collector
            .objects
            .iter()
            .find(|o| o.name == "LOCAL_OBJ")
            .unwrap();
        assert!(!local.is_module_level);
    }

    #[test]
    fn test_nested_array_in_method() {
        let code = r#"
            const GLOBAL = [{ key: "a" }];
            export function Component() {
                const LOCAL = [{ key: "b" }];  // Collected but not module-level
                return null;
            }
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.arrays.len(), 2);

        let global = collector
            .arrays
            .iter()
            .find(|a| a.name == "GLOBAL")
            .unwrap();
        assert!(global.is_module_level);

        let local = collector.arrays.iter().find(|a| a.name == "LOCAL").unwrap();
        assert!(!local.is_module_level);
    }

    #[test]
    fn test_deeply_nested_declarations_collected() {
        let code = r#"
            const GLOBAL = ["global"];
            function outer() {
                const OUTER = ["outer"];  // Collected, not module-level
                const inner = () => {
                    const INNER = ["inner"];  // Collected, not module-level
                };
            }
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.string_arrays.len(), 3);

        let global = collector
            .string_arrays
            .iter()
            .find(|a| a.name == "GLOBAL")
            .unwrap();
        assert!(global.is_module_level);

        let outer = collector
            .string_arrays
            .iter()
            .find(|a| a.name == "OUTER")
            .unwrap();
        assert!(!outer.is_module_level);

        let inner = collector
            .string_arrays
            .iter()
            .find(|a| a.name == "INNER")
            .unwrap();
        assert!(!inner.is_module_level);
    }

    // ============================================================
    // Translation prop collection tests
    // ============================================================

    #[test]
    fn test_collect_translation_prop_basic() {
        let code = r#"
            const t = useTranslations("MyNamespace");
            <MyComponent t={t} />;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(prop.component_name, "MyComponent");
        assert_eq!(prop.prop_name, "t");
        assert_eq!(prop.namespaces, vec![Some("MyNamespace".to_string())]);
    }

    #[test]
    fn test_collect_translation_prop_without_namespace() {
        let code = r#"
            const t = useTranslations();
            <MyComponent translate={t} />;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(prop.component_name, "MyComponent");
        assert_eq!(prop.prop_name, "translate");
        assert_eq!(prop.namespaces, vec![None]);
    }

    #[test]
    fn test_collect_translation_prop_with_await() {
        let code = r#"
            const t = await getTranslations("ServerNs");
            <ServerComponent t={t} />;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(prop.component_name, "ServerComponent");
        assert_eq!(prop.prop_name, "t");
        assert_eq!(prop.namespaces, vec![Some("ServerNs".to_string())]);
    }

    #[test]
    fn test_collect_multiple_translation_props() {
        let code = r#"
            const t1 = useTranslations("Namespace1");
            const t2 = useTranslations("Namespace2");
            <ComponentA t={t1} />;
            <ComponentB translate={t2} />;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_props.len(), 2);

        let prop_a = collector
            .translation_props
            .iter()
            .find(|p| p.component_name == "ComponentA")
            .unwrap();
        assert_eq!(prop_a.prop_name, "t");
        assert_eq!(prop_a.namespaces, vec![Some("Namespace1".to_string())]);

        let prop_b = collector
            .translation_props
            .iter()
            .find(|p| p.component_name == "ComponentB")
            .unwrap();
        assert_eq!(prop_b.prop_name, "translate");
        assert_eq!(prop_b.namespaces, vec![Some("Namespace2".to_string())]);
    }

    #[test]
    fn test_collect_same_component_multiple_namespaces() {
        // Same component called from different places with different namespaces
        let code = r#"
            function Page1() {
                const t = useTranslations("NS1");
                return <SharedComponent t={t} />;
            }
            function Page2() {
                const t = useTranslations("NS2");
                return <SharedComponent t={t} />;
            }
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(prop.component_name, "SharedComponent");
        assert_eq!(prop.prop_name, "t");
        assert_eq!(
            prop.namespaces,
            vec![Some("NS1".to_string()), Some("NS2".to_string())]
        );
    }

    #[test]
    fn test_skip_non_translation_prop() {
        let code = r#"
            const t = useTranslations("MyNs");
            const name = "John";
            <MyComponent t={t} name={name} />;
        "#;
        let collector = parse_and_collect(code);

        // Only 't' should be collected, not 'name'
        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(prop.prop_name, "t");
    }

    #[test]
    fn test_skip_lowercase_elements() {
        let code = r#"
            const t = useTranslations("MyNs");
            <div t={t} />;
        "#;
        let collector = parse_and_collect(code);

        // Lowercase elements are not user components, should be skipped
        assert!(collector.translation_props.is_empty());
    }

    #[test]
    fn test_collect_jsx_member_expression() {
        let code = r#"
            const t = useTranslations("MyNs");
            <UI.Button t={t} />;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(prop.component_name, "UI.Button");
        assert_eq!(prop.prop_name, "t");
    }

    #[test]
    fn test_collect_nested_jsx_children() {
        let code = r#"
            const t = useTranslations("MyNs");
            <Parent>
                <Child t={t} />
            </Parent>;
        "#;
        let collector = parse_and_collect(code);

        // Should collect from Child, not Parent
        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(prop.component_name, "Child");
    }

    #[test]
    fn test_translation_binding_tracking() {
        let code = r#"
            const t = useTranslations("NS1");
            const translate = useTranslations("NS2");
            const notTranslation = someOtherFunction();
        "#;
        let collector = parse_and_collect(code);

        // Should track both translation bindings
        assert_eq!(collector.translation_bindings_stack.len(), 1);
        let scope = collector.translation_bindings_stack.last().unwrap();
        assert_eq!(scope.len(), 2);
        assert_eq!(scope.get("t"), Some(&Some("NS1".to_string())));
        assert_eq!(scope.get("translate"), Some(&Some("NS2".to_string())));
        // Should not track non-translation function
        assert!(!scope.contains_key("notTranslation"));
    }

    #[test]
    fn test_translation_binding_scope_isolated() {
        let code = r#"
            function Page() {
                const t = useTranslations("Page");
                return null;
            }
            <Child t={t} />;
        "#;
        let collector = parse_and_collect(code);

        // Binding inside function should not leak to module-level JSX
        assert!(collector.translation_props.is_empty());
    }

    #[test]
    fn test_nested_arrow_binding_scope() {
        // Test that nested arrow functions have isolated scopes
        let code = r#"
            const outer = () => {
                const t = useTranslations("Outer");
                const inner = () => {
                    const t = useTranslations("Inner");
                    return <Child t={t} />;
                };
                return <Parent t={t} />;
            };
        "#;
        let collector = parse_and_collect(code);

        // Should collect both Parent and Child translation props
        assert_eq!(collector.translation_props.len(), 2);

        let parent = collector
            .translation_props
            .iter()
            .find(|p| p.component_name == "Parent")
            .unwrap();
        assert_eq!(parent.namespaces, vec![Some("Outer".to_string())]);

        let child = collector
            .translation_props
            .iter()
            .find(|p| p.component_name == "Child")
            .unwrap();
        assert_eq!(child.namespaces, vec![Some("Inner".to_string())]);
    }

    // ============================================================
    // Translation function call collection tests
    // ============================================================

    #[test]
    fn test_collect_translation_fn_call_basic() {
        let code = r#"
            const t = useTranslations("MyNamespace");
            const labels = usageTypeLabels(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let fn_call = &collector.translation_fn_calls[0];
        assert_eq!(fn_call.fn_name, "usageTypeLabels");
        assert_eq!(fn_call.fn_file_path, "test.ts"); // Same file
        assert_eq!(fn_call.arg_index, 0);
        assert_eq!(fn_call.namespaces, vec![Some("MyNamespace".to_string())]);
    }

    #[test]
    fn test_collect_translation_fn_call_without_namespace() {
        let code = r#"
            const t = useTranslations();
            const result = someHelper(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let fn_call = &collector.translation_fn_calls[0];
        assert_eq!(fn_call.fn_name, "someHelper");
        assert_eq!(fn_call.arg_index, 0);
        assert_eq!(fn_call.namespaces, vec![None]);
    }

    #[test]
    fn test_collect_translation_fn_call_with_await() {
        let code = r#"
            const t = await getTranslations("ServerNs");
            const labels = buildLabels(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let fn_call = &collector.translation_fn_calls[0];
        assert_eq!(fn_call.fn_name, "buildLabels");
        assert_eq!(fn_call.namespaces, vec![Some("ServerNs".to_string())]);
    }

    #[test]
    fn test_collect_translation_fn_call_second_argument() {
        let code = r#"
            const t = useTranslations("MyNs");
            const result = createLabels(config, t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let fn_call = &collector.translation_fn_calls[0];
        assert_eq!(fn_call.fn_name, "createLabels");
        assert_eq!(fn_call.arg_index, 1); // Second argument
        assert_eq!(fn_call.namespaces, vec![Some("MyNs".to_string())]);
    }

    #[test]
    fn test_collect_translation_fn_call_multiple_functions() {
        let code = r#"
            const t1 = useTranslations("NS1");
            const t2 = useTranslations("NS2");
            const labels1 = createLabels(t1);
            const labels2 = buildLabels(t2);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 2);

        let fn_call1 = collector
            .translation_fn_calls
            .iter()
            .find(|c| c.fn_name == "createLabels")
            .unwrap();
        assert_eq!(fn_call1.namespaces, vec![Some("NS1".to_string())]);

        let fn_call2 = collector
            .translation_fn_calls
            .iter()
            .find(|c| c.fn_name == "buildLabels")
            .unwrap();
        assert_eq!(fn_call2.namespaces, vec![Some("NS2".to_string())]);
    }

    #[test]
    fn test_collect_same_function_multiple_namespaces() {
        let code = r#"
            function Page1() {
                const t = useTranslations("NS1");
                return createLabels(t);
            }
            function Page2() {
                const t = useTranslations("NS2");
                return createLabels(t);
            }
        "#;
        let collector = parse_and_collect(code);

        // Same function called with different namespaces
        assert_eq!(collector.translation_fn_calls.len(), 1);
        let fn_call = &collector.translation_fn_calls[0];
        assert_eq!(fn_call.fn_name, "createLabels");
        assert_eq!(
            fn_call.namespaces,
            vec![Some("NS1".to_string()), Some("NS2".to_string())]
        );
    }

    #[test]
    fn test_skip_non_translation_function_call() {
        let code = r#"
            const t = useTranslations("MyNs");
            const name = "John";
            const result = someFunction(name);  // Not passing t
        "#;
        let collector = parse_and_collect(code);

        // Should not collect - the argument is not a translation function
        assert!(collector.translation_fn_calls.is_empty());
    }

    #[test]
    fn test_skip_translation_hook_call() {
        // Should not track useTranslations("Ns") itself as a function call
        let code = r#"
            const t = useTranslations("MyNs");
        "#;
        let collector = parse_and_collect(code);

        assert!(collector.translation_fn_calls.is_empty());
    }

    #[test]
    fn test_translation_fn_call_scope_isolated() {
        let code = r#"
            function Page() {
                const t = useTranslations("Page");
                return createLabels(t);
            }
            // t is not in scope here, so this should be a different variable
            const result = someFunc(t);
        "#;
        let collector = parse_and_collect(code);

        // Only the call inside Page() should be collected
        assert_eq!(collector.translation_fn_calls.len(), 1);
        let fn_call = &collector.translation_fn_calls[0];
        assert_eq!(fn_call.fn_name, "createLabels");
    }

    #[test]
    fn test_translation_fn_call_in_nested_arrow() {
        let code = r#"
            const Component = () => {
                const t = useTranslations("Outer");
                const inner = () => {
                    const t = useTranslations("Inner");
                    return buildLabels(t);
                };
                return createLabels(t);
            };
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 2);

        let outer_call = collector
            .translation_fn_calls
            .iter()
            .find(|c| c.fn_name == "createLabels")
            .unwrap();
        assert_eq!(outer_call.namespaces, vec![Some("Outer".to_string())]);

        let inner_call = collector
            .translation_fn_calls
            .iter()
            .find(|c| c.fn_name == "buildLabels")
            .unwrap();
        assert_eq!(inner_call.namespaces, vec![Some("Inner".to_string())]);
    }
}

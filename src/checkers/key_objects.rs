use std::{collections::HashMap, path::Path};

use swc_ecma_ast::{
    ArrayLit, ArrowExpr, Decl, Expr, ExprOrSpread, Function, ImportSpecifier, Lit,
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

/// Collects object literals, arrays, and imports from TypeScript/TSX files
pub struct KeyObjectCollector {
    pub file_path: String,
    pub objects: Vec<KeyObject>,
    pub arrays: Vec<KeyArray>,
    pub string_arrays: Vec<StringArray>,
    pub imports: FileImports,
    /// Tracks nesting depth: 0 = module level, >0 = inside function/arrow
    scope_depth: usize,
}

impl KeyObjectCollector {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            objects: Vec::new(),
            arrays: Vec::new(),
            string_arrays: Vec::new(),
            imports: Vec::new(),
            scope_depth: 0,
        }
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
}

impl Visit for KeyObjectCollector {
    fn visit_function(&mut self, node: &Function) {
        self.scope_depth += 1;
        node.visit_children_with(self);
        self.scope_depth -= 1;
    }

    fn visit_arrow_expr(&mut self, node: &ArrowExpr) {
        self.scope_depth += 1;
        node.visit_children_with(self);
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
}

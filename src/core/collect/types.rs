//! Type definitions for Phase 1: Collection.
//!
//! This module defines the data structures used to collect information about
//! translation keys, translation functions, and related code patterns during
//! the initial scan of the codebase.
//!
//! The actual collection logic is in `registry_collector.rs` - this module
//! contains only type definitions and utility functions.
//!
//! # Phase Context
//!
//! All types in this module are **created in Phase 1** by `RegistryCollector`
//! and **consumed in Phase 2** by `FileAnalyzer` to resolve translation calls.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use swc_ecma_ast::{ObjectPatProp, Pat};

use crate::core::schema::SchemaRegistry;

/// Object literal containing translation key candidates.
///
/// Used for patterns like:
/// ```typescript
/// const toolKeys = {
///   create: "tools.create",
///   edit: "tools.edit"
/// };
/// ```
///
/// Enables resolution of dynamic keys like `t(toolKeys[action])` in Phase 2.
#[derive(Debug, Clone)]
pub struct KeyObject {
    /// Variable name (e.g., "toolKeys").
    pub name: String,
    /// File where the object is defined (relative to source root).
    pub file_path: String,
    /// Whether the object is exported (affects cross-file resolution).
    pub is_exported: bool,
    /// Whether defined at module level (scope_depth == 0).
    /// Non-module-level objects are collected but have lower priority.
    pub is_module_level: bool,
    /// All string values from the object (e.g., ["tools.create", "tools.edit"]).
    pub candidate_keys: Vec<String>,
}

/// Registry of all objects that may contain translation key mappings.
///
/// **Phase 1**: Created by `RegistryCollector`
/// **Phase 2**: Used by `ValueAnalyzer` to resolve `obj[key]` expressions
///
/// **Key format**: `"file_path:object_name"` (e.g., `"src/utils/keys.ts:toolKeys"`)
pub type KeyObjectRegistry = HashMap<String, KeyObject>;

/// Import statement information for cross-file resolution.
///
/// Tracks how names are imported so we can resolve references like:
/// ```typescript
/// import { toolKeys as keys } from "./constants";
/// t(keys[action]); // Resolve 'keys' back to 'toolKeys' in "./constants"
/// ```
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Local name in the importing file (e.g., "keys").
    pub local_name: String,
    /// Original name in the imported file (e.g., "toolKeys", or "default" for default imports).
    pub imported_name: String,
    /// Import source path (e.g., "./constants").
    pub module_path: String,
}

/// All imports for a single file.
///
/// **Phase 1**: Created by `RegistryCollector`
/// **Phase 2**: Used by `ValueAnalyzer` to resolve imported variable references
pub type FileImports = Vec<ImportInfo>;

/// Array of objects containing translation key candidates.
///
/// Used for patterns like:
/// ```typescript
/// const capabilities = [
///   { titleKey: "ai.features.novel", descKey: "ai.features.novel.desc" },
///   { titleKey: "ai.features.char", descKey: "ai.features.char.desc" }
/// ];
/// ```
///
/// Enables resolution of patterns like `capabilities.map(c => t(c.titleKey))` in Phase 2.
#[derive(Debug, Clone)]
pub struct KeyArray {
    /// Variable name (e.g., "capabilities").
    pub name: String,
    /// File where the array is defined (relative to source root).
    pub file_path: String,
    /// Whether the array is exported (affects cross-file resolution).
    pub is_exported: bool,
    /// Whether defined at module level (scope_depth == 0).
    /// Non-module-level arrays are collected but have lower priority.
    pub is_module_level: bool,
    /// Maps property names to all values found in array elements.
    /// Example: `{ "titleKey": ["ai.features.novel", "ai.features.char"], "descKey": [...] }`
    pub property_values: HashMap<String, Vec<String>>,
}

/// Registry of all arrays that may contain translation key mappings.
///
/// **Phase 1**: Created by `RegistryCollector`
/// **Phase 2**: Used by `ValueAnalyzer` to resolve `arr.map(item => item.prop)` patterns
///
/// **Key format**: `"file_path:array_name"` (e.g., `"src/config.ts:capabilities"`)
pub type KeyArrayRegistry = HashMap<String, KeyArray>;

/// String array containing translation key candidates.
///
/// Used for patterns like:
/// ```typescript
/// const FEATURE_KEYS = ["save", "characters", "chapters"] as const;
/// ```
///
/// Enables resolution of patterns like `FEATURE_KEYS.map(k => t(\`features.\${k}\`))` in Phase 2.
#[derive(Debug, Clone)]
pub struct StringArray {
    /// Variable name (e.g., "FEATURE_KEYS").
    pub name: String,
    /// File where the array is defined (relative to source root).
    pub file_path: String,
    /// Whether the array is exported (affects cross-file resolution).
    pub is_exported: bool,
    /// Whether defined at module level (scope_depth == 0).
    /// Non-module-level arrays are collected but have lower priority.
    pub is_module_level: bool,
    /// All string elements in the array (e.g., ["save", "characters", "chapters"]).
    pub values: Vec<String>,
}

/// Registry of all string arrays that may contain translation keys.
///
/// **Phase 1**: Created by `RegistryCollector`
/// **Phase 2**: Used by `ValueAnalyzer` to resolve `arr.map(k => k)` or `arr[0]` patterns
///
/// **Key format**: `"file_path:array_name"` (e.g., `"src/constants.ts:FEATURE_KEYS"`)
pub type StringArrayRegistry = HashMap<String, StringArray>;

/// Translation function passed as a JSX prop.
///
/// Used for patterns like:
/// ```typescript
/// const t = useTranslations("MyNamespace");
/// <MyComponent t={t} />
/// ```
///
/// In Phase 2, when analyzing `MyComponent`, we check if it receives a translation
/// prop and register the prop parameter as a translation binding.
#[derive(Debug, Clone)]
pub struct TranslationProp {
    /// Component name receiving the prop (e.g., "MyComponent", "UI.Button").
    pub component_name: String,
    /// Prop name (e.g., "t", "translate").
    pub prop_name: String,
    /// All possible namespaces from different call sites.
    /// `None` means no namespace (e.g., `useTranslations()` with no argument).
    /// Multiple entries indicate the component is used with different namespaces.
    pub namespaces: Vec<Option<String>>,
}

/// Registry of translation functions passed as JSX props.
///
/// **Phase 1**: Created by `RegistryCollector` when it sees `<Component t={translationVar} />`
/// **Phase 2**: Used by `FileAnalyzer` to register translation bindings from component props
///
/// **Key format**: `"ComponentName:propName"` (e.g., `"MyComponent:t"`, `"UI.Button:translate"`)
pub type TranslationPropRegistry = HashMap<String, TranslationProp>;

/// Create registry key for `TranslationPropRegistry`.
///
/// **Format**: `"ComponentName:propName"`
///
/// # Examples
///
/// ```
/// # use glot::core::collect::types::make_translation_prop_key;
/// assert_eq!(make_translation_prop_key("MyComponent", "t"), "MyComponent:t");
/// assert_eq!(make_translation_prop_key("UI.Button", "translate"), "UI.Button:translate");
/// ```
pub fn make_translation_prop_key(component_name: &str, prop_name: &str) -> String {
    format!("{}.{}", component_name, prop_name)
}

/// Translation function passed as a regular function call argument.
///
/// Used for patterns like:
/// ```typescript
/// const t = useTranslations("MyNamespace");
/// myHelper(t);  // Pass translation function to helper
/// ```
///
/// Enables tracking translation keys in utility/factory functions that receive
/// translation functions as parameters, not just React components.
///
/// In Phase 2, when analyzing the definition of `myHelper`, we check if it receives
/// a translation function argument and register the parameter as a translation binding.
#[derive(Debug, Clone)]
pub struct TranslationFnCall {
    /// File where the called function is defined (relative to source root).
    pub fn_file_path: String,
    /// Function name (e.g., "myHelper", or "default" for default exports).
    pub fn_name: String,
    /// Argument index (0-based) where the translation function is passed.
    pub arg_index: usize,
    /// All possible namespaces from different call sites.
    /// `None` means no namespace (e.g., `useTranslations()` with no argument).
    /// Multiple entries indicate the function is called with different namespaces.
    pub namespaces: Vec<Option<String>>,
}

/// Registry of translation functions passed as regular function call arguments.
///
/// **Phase 1**: Created by `RegistryCollector` when it sees `someFunc(translationVar)`
/// **Phase 2**: Used by `FileAnalyzer` to register translation bindings from function parameters
///
/// **Key format**: `"file_path:fn_name:arg_index"` (e.g., `"src/utils.ts:myHelper:0"`)
pub type TranslationFnCallRegistry = HashMap<String, TranslationFnCall>;

/// Create registry key for `TranslationFnCallRegistry`.
///
/// **Format**: `"file_path:fn_name:arg_index"`
///
/// # Examples
///
/// ```
/// # use glot::core::collect::types::make_translation_fn_call_key;
/// assert_eq!(
///     make_translation_fn_call_key("src/utils.ts", "myHelper", 0),
///     "src/utils.ts:myHelper:0"
/// );
/// assert_eq!(
///     make_translation_fn_call_key("src/lib.ts", "default", 1),
///     "src/lib.ts:default:1"
/// );
/// ```
pub fn make_translation_fn_call_key(fn_file_path: &str, fn_name: &str, arg_index: usize) -> String {
    format!("{}.{}.{}", fn_file_path, fn_name, arg_index)
}

/// Binding value in the translation bindings stack (used during Phase 1 collection).
///
/// Used by `RegistryCollector` to track which variables are translation functions
/// and detect shadowing within nested scopes.
#[derive(Debug, Clone)]
pub enum TranslationBindingValue {
    /// A translation function binding with optional namespace.
    /// Example: `const t = useTranslations("MyNs")` → `Translation(Some("MyNs"))`
    Translation(Option<String>),

    /// A shadowed binding (parameter that shadows outer translation binding).
    ///
    /// Example:
    /// ```typescript
    /// const t = useTranslations("Outer");
    /// function inner(t) {  // This 't' shadows the outer 't'
    ///   // ...
    /// }
    /// ```
    ///
    /// When encountered during lookup, indicates "stop searching, this is not a translation".
    Shadowed,
}

/// Extract all identifier names from a pattern (handles destructuring).
/// Supports: simple ident, object destructuring, array destructuring,
/// default values, rest patterns, and renamed destructuring.
pub fn extract_binding_names(pat: &Pat) -> Vec<String> {
    match pat {
        Pat::Ident(ident) => vec![ident.id.sym.to_string()],
        Pat::Object(obj) => obj
            .props
            .iter()
            .flat_map(|prop| match prop {
                ObjectPatProp::KeyValue(kv) => extract_binding_names(&kv.value),
                ObjectPatProp::Assign(assign) => vec![assign.key.sym.to_string()],
                ObjectPatProp::Rest(rest) => extract_binding_names(&rest.arg),
            })
            .collect(),
        Pat::Array(arr) => arr
            .elems
            .iter()
            .flatten()
            .flat_map(extract_binding_names)
            .collect(),
        Pat::Assign(assign) => extract_binding_names(&assign.left),
        Pat::Rest(rest) => extract_binding_names(&rest.arg),
        _ => vec![],
    }
}

/// Resolves a TypeScript import path to an actual file path
pub fn resolve_import_path(current_file: &Path, import_path: &str) -> Option<String> {
    // Only handle relative imports
    if !import_path.starts_with('.') {
        return None;
    }

    let base_dir = current_file.parent()?;
    // Strip leading "./" from import_path to avoid paths like "./src/./utils"
    let normalized_import = import_path.strip_prefix("./").unwrap_or(import_path);
    let resolved = base_dir.join(normalized_import);

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

/// Create a generic registry key for file-scoped items.
///
/// Used for `KeyObjectRegistry`, `KeyArrayRegistry`, and `StringArrayRegistry`.
///
/// **Format**: `"file_path:name"`
///
/// # Examples
///
/// ```
/// # use glot::core::collect::types::make_registry_key;
/// assert_eq!(
///     make_registry_key("src/constants.ts", "FEATURE_KEYS"),
///     "src/constants.ts:FEATURE_KEYS"
/// );
/// ```
pub fn make_registry_key(file_path: &str, name: &str) -> String {
    format!("{}.{}", file_path, name)
}

// ============================================================
// Comment Types (glot-disable, glot-message-keys)
// ============================================================

/// Rules that can be suppressed via glot comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum SuppressibleRule {
    Hardcoded,
    Untranslated,
}

/// Range representing disabled lines [start, end] inclusive.
#[derive(Debug, Clone, Copy)]
pub struct DisabledRange {
    pub start: usize,
    pub end: usize, // usize::MAX for open-ended
}

/// Tracks suppressed lines per rule for a single file.
#[derive(Debug, Default)]
pub struct Suppressions {
    /// Single-line suppressions: rule -> set of line numbers
    pub disabled_lines: HashMap<SuppressibleRule, HashSet<usize>>,
    /// Range-based suppressions: rule -> list of ranges
    pub disabled_ranges: HashMap<SuppressibleRule, Vec<DisabledRange>>,
}

/// Key declaration for a single line.
#[derive(Debug, Clone)]
pub struct KeyDeclaration {
    /// Absolute patterns (e.g., "Common.key", "errors.*")
    /// These patterns are fully qualified and ready for expansion.
    pub absolute_patterns: Vec<String>,
    /// Relative patterns (starting with `.`) that need namespace expansion.
    /// e.g., `.features.*.title` will become `Namespace.features.*.title`
    pub relative_patterns: Vec<String>,
}

/// Stores parsed glot-message-keys declarations for a file.
#[derive(Debug, Default)]
pub struct Declarations {
    /// Line number -> key declaration
    pub entries: HashMap<usize, KeyDeclaration>,
}

/// All glot comments collected from a single file.
///
/// **Phase 1**: Collected by comment parsers alongside other registries
/// **Phase 2**: Passed to `FileAnalyzer` to check suppressions and expand declarations
#[derive(Debug, Default)]
pub struct FileComments {
    /// Suppression directives (glot-disable, glot-enable, glot-disable-next-line).
    /// Checked during Phase 2 to skip reporting suppressed issues.
    pub suppressions: Suppressions,

    /// Key declarations (glot-message-keys: Common.*, .features.*).
    /// Expanded during Phase 3 to validate declared keys exist in locale files.
    pub declarations: Declarations,
}

/// All file comments across the codebase, indexed by file path.
///
/// **Phase 1**: Created by comment parsers for each file
/// **Phase 2-3**: Used to check suppressions and validate declarations
///
/// **Key format**: File path (relative to source root)
pub type AllFileComments = HashMap<String, FileComments>;

// ============================================================
// Aggregated Registry Types
// ============================================================

/// Aggregated registry of all collected symbols from Phase 1.
///
/// This struct holds all cross-file dependency information collected during
/// the initial scan of the codebase. It's created by scanning all files in
/// Phase 1 and passed to `FileAnalyzer` in Phase 2 for resolving dynamic keys
/// and translation function bindings.
///
/// **Note**: File imports are stored separately in `AllFileImports`, not in this struct.
pub struct Registries {
    /// Schema function registry (e.g., `loginSchema`, `profileSchema`).
    /// Used to detect validation schema calls that use translation functions.
    pub schema: SchemaRegistry,

    /// Object literal registry (key objects with string values).
    /// Used to resolve `obj[key]` expressions in translation calls.
    pub key_object: KeyObjectRegistry,

    /// Object array registry (arrays of objects with properties).
    /// Used to resolve `arr.map(item => item.prop)` expressions.
    pub key_array: KeyArrayRegistry,

    /// String array registry (arrays of string literals).
    /// Used to resolve `arr.map(k => k)` or `arr[0]` expressions.
    pub string_array: StringArrayRegistry,

    /// Translation functions passed as JSX props.
    /// Used to register translation bindings when entering component definitions.
    pub translation_prop: TranslationPropRegistry,

    /// Translation functions passed as regular function call arguments.
    /// Used to register translation bindings when entering function definitions.
    pub translation_fn_call: TranslationFnCallRegistry,

    /// Maps file_path → default export name for files with default exports.
    /// Used to match translation function calls with default imported functions.
    /// Example: `{ "src/utils.ts": "myHelper" }` means `utils.ts` exports `myHelper` as default.
    pub default_exports: HashMap<String, String>,
}

/// All file imports across the codebase, indexed by file path.
///
/// **Phase 1**: Created by `RegistryCollector` for each file
/// **Phase 2**: Used by `ValueAnalyzer` to resolve imported variable references
///
/// **Key format**: File path (relative to source root)
/// **Value**: List of all imports in that file
pub type AllFileImports = HashMap<String, FileImports>;

#[cfg(test)]
mod tests {
    use crate::core::collect::RegistryCollector;
    use swc_common::FileName;
    use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};
    use swc_ecma_visit::VisitWith;

    fn parse_and_collect(code: &str) -> RegistryCollector {
        let source_map = swc_common::SourceMap::default();
        let source_file =
            source_map.new_source_file(FileName::Real("test.ts".into()).into(), code.to_string());

        let syntax = Syntax::Typescript(TsSyntax {
            tsx: true,
            ..Default::default()
        });
        let mut parser = Parser::new(syntax, StringInput::from(&*source_file), None);
        let module = parser.parse_module().unwrap();

        let mut collector = RegistryCollector::new("test.ts");
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

    // ============================================================
    // Translation function call tests
    // ============================================================

    #[test]
    fn test_collect_translation_fn_call_basic() {
        let code = r#"
            const t = useTranslations("MyNs");
            someFunction(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(call.fn_name, "someFunction");
        assert_eq!(call.arg_index, 0);
        assert_eq!(call.namespaces, vec![Some("MyNs".to_string())]);
    }

    #[test]
    fn test_collect_translation_fn_call_second_argument() {
        let code = r#"
            const t = useTranslations("MyNs");
            someFunction("first", t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(call.arg_index, 1);
    }

    #[test]
    fn test_collect_translation_fn_call_without_namespace() {
        let code = r#"
            const t = useTranslations();
            myHelper(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(call.namespaces, vec![None]);
    }

    #[test]
    fn test_collect_translation_fn_call_with_await() {
        let code = r#"
            const t = await getTranslations("ServerNs");
            processTranslations(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(call.fn_name, "processTranslations");
        assert_eq!(call.namespaces, vec![Some("ServerNs".to_string())]);
    }

    #[test]
    fn test_collect_translation_fn_call_multiple_functions() {
        let code = r#"
            const t = useTranslations("MyNs");
            helperA(t);
            helperB(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 2);
        assert!(
            collector
                .translation_fn_calls
                .iter()
                .any(|c| c.fn_name == "helperA")
        );
        assert!(
            collector
                .translation_fn_calls
                .iter()
                .any(|c| c.fn_name == "helperB")
        );
    }

    #[test]
    fn test_collect_same_function_multiple_namespaces() {
        let code = r#"
            function Page1() {
                const t = useTranslations("NS1");
                sharedHelper(t);
            }
            function Page2() {
                const t = useTranslations("NS2");
                sharedHelper(t);
            }
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(call.fn_name, "sharedHelper");
        assert_eq!(
            call.namespaces,
            vec![Some("NS1".to_string()), Some("NS2".to_string())]
        );
    }

    #[test]
    fn test_skip_translation_hook_call() {
        let code = r#"
            const t = useTranslations("MyNs");
            // Should NOT track the hook call itself
        "#;
        let collector = parse_and_collect(code);

        // No function calls should be tracked, only the translation binding
        assert!(collector.translation_fn_calls.is_empty());
    }

    #[test]
    fn test_skip_non_translation_function_call() {
        let code = r#"
            const t = useTranslations("MyNs");
            const name = "John";
            someFunction(name);  // 'name' is not a translation function
        "#;
        let collector = parse_and_collect(code);

        // Should not track calls with non-translation arguments
        assert!(collector.translation_fn_calls.is_empty());
    }

    // ============================================================
    // Default export tracking tests
    // ============================================================

    #[test]
    fn test_default_export_function_decl() {
        let code = r#"
            export default function MyComponent() {
                return null;
            }
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(
            collector.default_export_name,
            Some("MyComponent".to_string())
        );
    }

    #[test]
    fn test_default_export_arrow_function() {
        let code = r#"
            export default () => null;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.default_export_name, Some("default".to_string()));
    }

    #[test]
    fn test_default_export_expr() {
        let code = r#"
            const MyComponent = () => null;
            export default MyComponent;
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(
            collector.default_export_name,
            Some("MyComponent".to_string())
        );
    }

    #[test]
    fn test_no_default_export() {
        let code = r#"
            export const foo = "bar";
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.default_export_name, None);
    }

    // ============================================================
    // Translation binding scope tests
    // ============================================================

    #[test]
    fn test_translation_binding_tracking() {
        let code = r#"
            const t = useTranslations("NS1");
            const translate = useTranslations("NS2");
            const notTranslation = someOtherFunction();
        "#;
        let collector = parse_and_collect(code);

        // Should track both translation bindings but not the third one
        // (We can't directly test internal bindings, so we test side effects)
        assert!(collector.translation_props.is_empty()); // No props used
        assert!(collector.translation_fn_calls.is_empty()); // No calls made
    }

    #[test]
    fn test_translation_binding_scope_isolated() {
        let code = r#"
            function ComponentA() {
                const t = useTranslations("NSA");
                <Child t={t} />;
            }
            function ComponentB() {
                const t = useTranslations("NSB");
                <Child t={t} />;
            }
        "#;
        let collector = parse_and_collect(code);

        // Should track both namespaces for the same Child component
        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(
            prop.namespaces,
            vec![Some("NSA".to_string()), Some("NSB".to_string())]
        );
    }

    #[test]
    fn test_nested_arrow_binding_scope() {
        let code = r#"
            const outer = () => {
                const t = useTranslations("Outer");
                const inner = () => {
                    // 't' from outer scope should be accessible
                    helperFunc(t);
                };
            };
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(call.namespaces, vec![Some("Outer".to_string())]);
    }

    #[test]
    fn test_translation_fn_call_parameter_shadows_outer_binding() {
        let code = r#"
            const outer = (t) => {
                const inner = (t) => {
                    // This 't' is shadowed by inner's parameter
                    someFunc(t);
                };
            };
        "#;
        let collector = parse_and_collect(code);

        // Should not track this call because 't' is shadowed
        assert!(collector.translation_fn_calls.is_empty());
    }

    #[test]
    fn test_translation_fn_call_function_param_shadows() {
        let code = r#"
            function outer() {
                const t = useTranslations("Outer");
                function inner(t) {
                    // This 't' is shadowed by inner's parameter
                    someFunc(t);
                }
            }
        "#;
        let collector = parse_and_collect(code);

        // Should not track because 't' is shadowed
        assert!(collector.translation_fn_calls.is_empty());
    }

    #[test]
    fn test_translation_fn_call_destructured_param_shadows() {
        let code = r#"
            function outer() {
                const t = useTranslations("Outer");
                function inner({ t }) {
                    // This 't' is shadowed by destructured parameter
                    someFunc(t);
                }
            }
        "#;
        let collector = parse_and_collect(code);

        // Should not track because 't' is shadowed by destructuring
        assert!(collector.translation_fn_calls.is_empty());
    }

    #[test]
    fn test_translation_fn_call_renamed_destructured_param_shadows() {
        let code = r#"
            function outer() {
                const t = useTranslations("Outer");
                function inner({ translate: t }) {
                    // This 't' is shadowed by renamed destructured parameter
                    someFunc(t);
                }
            }
        "#;
        let collector = parse_and_collect(code);

        // Should not track because 't' is shadowed
        assert!(collector.translation_fn_calls.is_empty());
    }

    #[test]
    fn test_translation_fn_call_scope_isolated() {
        let code = r#"
            function A() {
                const t = useTranslations("NSA");
                helper(t);
            }
            function B() {
                const t = useTranslations("NSB");
                helper(t);
            }
        "#;
        let collector = parse_and_collect(code);

        // Should merge both namespaces for the same helper function
        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(
            call.namespaces,
            vec![Some("NSA".to_string()), Some("NSB".to_string())]
        );
    }

    #[test]
    fn test_translation_fn_call_in_nested_arrow() {
        let code = r#"
            const component = () => {
                const t = useTranslations("MyNs");
                const processData = () => {
                    dataHelper(t);
                };
            };
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(call.fn_name, "dataHelper");
        assert_eq!(call.namespaces, vec![Some("MyNs".to_string())]);
    }

    #[test]
    fn test_translation_fn_call_with_import_alias() {
        let code = r#"
            import { myHelper as helper } from "./utils";
            const t = useTranslations("MyNs");
            helper(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        // Should use original imported name
        assert_eq!(call.fn_name, "myHelper");
    }

    #[test]
    fn test_translation_fn_call_with_default_import() {
        let code = r#"
            import myHelper from "./utils";
            const t = useTranslations("MyNs");
            myHelper(t);
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_fn_calls.len(), 1);
        let call = &collector.translation_fn_calls[0];
        assert_eq!(call.fn_name, "default");
    }

    #[test]
    fn test_translation_prop_arrow_function_component() {
        let code = r#"
            const MyComponent = () => {
                const t = useTranslations("MyNs");
                return <Child t={t} />;
            };
        "#;
        let collector = parse_and_collect(code);

        assert_eq!(collector.translation_props.len(), 1);
        let prop = &collector.translation_props[0];
        assert_eq!(prop.component_name, "Child");
    }
}

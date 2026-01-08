//! Value analyzer for dynamic translation key expressions.
//!
//! This module provides the core analysis engine that transforms AST expressions
//! into `ValueSource` representations. It tracks variable bindings, iterator
//! parameters, and performs registry lookups to resolve dynamic keys.

use std::{collections::HashMap, path::Path};

use swc_ecma_ast::{BinExpr, BinaryOp, CondExpr, Expr, Ident, Lit, MemberExpr, MemberProp, Tpl};

use crate::checkers::{
    key_objects::{
        FileImports, KeyArrayRegistry, KeyObjectRegistry, StringArrayRegistry, make_registry_key,
        resolve_import_path,
    },
    value_source::{UnresolvableReason, ValueSource},
};

/// Get the type name of an expression for error messages.
fn expr_type_name(expr: &Expr) -> String {
    match expr {
        Expr::Array(_) => "Array",
        Expr::Arrow(_) => "Arrow",
        Expr::Assign(_) => "Assign",
        Expr::Await(_) => "Await",
        Expr::Bin(_) => "Bin",
        Expr::Call(_) => "Call",
        Expr::Class(_) => "Class",
        Expr::Cond(_) => "Cond",
        Expr::Fn(_) => "Fn",
        Expr::Ident(_) => "Ident",
        Expr::Invalid(_) => "Invalid",
        Expr::Lit(_) => "Lit",
        Expr::Member(_) => "Member",
        Expr::MetaProp(_) => "MetaProp",
        Expr::New(_) => "New",
        Expr::Object(_) => "Object",
        Expr::OptChain(_) => "OptChain",
        Expr::Paren(_) => "Paren",
        Expr::PrivateName(_) => "PrivateName",
        Expr::Seq(_) => "Seq",
        Expr::SuperProp(_) => "SuperProp",
        Expr::TaggedTpl(_) => "TaggedTpl",
        Expr::This(_) => "This",
        Expr::Tpl(_) => "Tpl",
        Expr::TsAs(_) => "TsAs",
        Expr::TsConstAssertion(_) => "TsConstAssertion",
        Expr::TsInstantiation(_) => "TsInstantiation",
        Expr::TsNonNull(_) => "TsNonNull",
        Expr::TsSatisfies(_) => "TsSatisfies",
        Expr::TsTypeAssertion(_) => "TsTypeAssertion",
        Expr::Unary(_) => "Unary",
        Expr::Update(_) => "Update",
        Expr::Yield(_) => "Yield",
        Expr::JSXMember(_) => "JSXMember",
        Expr::JSXNamespacedName(_) => "JSXNamespacedName",
        Expr::JSXEmpty(_) => "JSXEmpty",
        Expr::JSXElement(_) => "JSXElement",
        Expr::JSXFragment(_) => "JSXFragment",
    }
    .to_string()
}

/// Binding type for iterator variables
#[derive(Debug, Clone)]
pub enum IteratorBindingType {
    /// Iterating over string array directly: `KEYS.map(k => t(k))`
    StringArrayDirect,
    /// Iterating over object array, accessing properties: `items.map(i => t(i.key))`
    ObjectArrayProperty,
}

/// Information about an iterator variable binding
#[derive(Debug, Clone)]
pub struct IteratorBinding {
    pub array_name: String,
    pub binding_type: IteratorBindingType,
}

/// Analyzer for resolving dynamic translation key expressions.
///
/// This struct holds references to all registries and tracks variable bindings
/// to enable resolution of dynamic keys.
pub struct ValueAnalyzer<'a> {
    file_path: &'a str,

    /// Registry of objects: `{ name: "value" }`
    key_object_registry: &'a KeyObjectRegistry,

    /// Registry of object arrays: `[{ prop: "value" }]`
    key_array_registry: &'a KeyArrayRegistry,

    /// Registry of string arrays: `["a", "b"]`
    string_array_registry: &'a StringArrayRegistry,

    /// Import information for cross-file resolution
    file_imports: &'a FileImports,

    /// Maps variable name to its resolved ValueSource
    /// e.g., `{ "key": ObjectAccess { object_name: "toolKeys" } }`
    variable_bindings: HashMap<String, ValueSource>,

    /// Stack of iterator binding scopes (innermost last)
    /// Each scope maps iterator parameter names to their bindings
    /// e.g., `[{ "k": IteratorBinding { array_name: "KEYS", ... } }]`
    iterator_scopes: Vec<HashMap<String, IteratorBinding>>,
}

impl<'a> ValueAnalyzer<'a> {
    /// Create a new ValueAnalyzer with the given registries.
    pub fn new(
        file_path: &'a str,
        key_object_registry: &'a KeyObjectRegistry,
        key_array_registry: &'a KeyArrayRegistry,
        string_array_registry: &'a StringArrayRegistry,
        file_imports: &'a FileImports,
    ) -> Self {
        Self {
            file_path,
            key_object_registry,
            key_array_registry,
            string_array_registry,
            file_imports,
            variable_bindings: HashMap::new(),
            // Start with one global scope
            iterator_scopes: vec![HashMap::new()],
        }
    }

    /// Enter a new scope for iterator bindings.
    /// Call this when entering a callback body (e.g., map/forEach arrow function).
    pub fn enter_scope(&mut self) {
        self.iterator_scopes.push(HashMap::new());
    }

    /// Exit the current scope for iterator bindings.
    /// Call this when leaving a callback body.
    pub fn exit_scope(&mut self) {
        // Keep at least the global scope
        if self.iterator_scopes.len() > 1 {
            self.iterator_scopes.pop();
        }
    }

    /// Look up an iterator binding by name, searching from innermost to outermost scope.
    fn get_iterator_binding(&self, name: &str) -> Option<&IteratorBinding> {
        for scope in self.iterator_scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(binding);
            }
        }
        None
    }

    /// Main entry point: analyze an expression and return its ValueSource.
    pub fn analyze_expr(&self, expr: &Expr) -> ValueSource {
        match crate::checkers::unwrap_paren(expr) {
            // Static string: "key"
            Expr::Lit(Lit::Str(s)) => match s.value.as_str() {
                Some(v) => ValueSource::Literal(v.to_string()),
                None => ValueSource::Unresolvable {
                    reason: UnresolvableReason::UnsupportedExpression {
                        expr_type: "Str (non-UTF8)".to_string(),
                    },
                },
            },

            // Number literal: for cases like `step${5}.title`
            Expr::Lit(Lit::Num(n)) => ValueSource::Literal(n.value.to_string()),

            // Template literal: `prefix.${expr}.suffix`
            Expr::Tpl(tpl) => self.analyze_template(tpl),

            // Conditional: cond ? a : b
            Expr::Cond(cond) => self.analyze_conditional(cond),

            // Binary expression: a || b (logical or)
            Expr::Bin(bin) => self.analyze_binary(bin),

            // Identifier: might be a tracked variable or iterator param
            Expr::Ident(ident) => self.analyze_ident(ident),

            // Member access: obj.prop or item.key in iteration
            Expr::Member(member) => self.analyze_member(member),

            other => ValueSource::Unresolvable {
                reason: UnresolvableReason::UnsupportedExpression {
                    expr_type: expr_type_name(other),
                },
            },
        }
    }

    /// Analyze a template literal expression.
    fn analyze_template(&self, tpl: &Tpl) -> ValueSource {
        // No expressions: static template
        if tpl.exprs.is_empty() {
            return tpl
                .quasis
                .first()
                .and_then(|q| q.cooked.as_ref())
                .and_then(|s| s.as_str())
                .map(|s| ValueSource::Literal(s.to_string()))
                .unwrap_or(ValueSource::Unresolvable {
                    reason: UnresolvableReason::UnsupportedExpression {
                        expr_type: "Tpl (empty quasis)".to_string(),
                    },
                });
        }

        // Multiple expressions in template: `${a}.${b}.${c}`
        // Currently unsupported - would need cartesian product of all possible values.
        // Return early so the single-expression handling below can safely assume
        // exactly 2 quasis (prefix + suffix).
        if tpl.exprs.len() > 1 {
            return ValueSource::Unresolvable {
                reason: UnresolvableReason::ComplexTemplate {
                    expr_count: tpl.exprs.len(),
                },
            };
        }

        // Single expression: `prefix.${expr}.suffix`
        let prefix = tpl
            .quasis
            .first()
            .and_then(|q| q.cooked.as_ref())
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let suffix = tpl
            .quasis
            .get(1)
            .and_then(|q| q.cooked.as_ref())
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let inner = self.analyze_expr(&tpl.exprs[0]);

        ValueSource::Template {
            prefix,
            suffix,
            inner: Box::new(inner),
        }
    }

    /// Analyze a conditional expression.
    fn analyze_conditional(&self, cond: &CondExpr) -> ValueSource {
        let consequent = self.analyze_expr(&cond.cons);
        let alternate = self.analyze_expr(&cond.alt);

        ValueSource::Conditional {
            consequent: Box::new(consequent),
            alternate: Box::new(alternate),
        }
    }

    /// Analyze a binary expression (currently only handles logical OR).
    fn analyze_binary(&self, bin: &BinExpr) -> ValueSource {
        match bin.op {
            // Logical OR: a || b -> treat as conditional
            BinaryOp::LogicalOr => {
                let left = self.analyze_expr(&bin.left);
                let right = self.analyze_expr(&bin.right);
                ValueSource::Conditional {
                    consequent: Box::new(left),
                    alternate: Box::new(right),
                }
            }
            // Other binary ops are not supported
            op => ValueSource::Unresolvable {
                reason: UnresolvableReason::UnsupportedExpression {
                    expr_type: format!("BinaryOp::{:?}", op),
                },
            },
        }
    }

    /// Analyze an identifier expression.
    fn analyze_ident(&self, ident: &Ident) -> ValueSource {
        let name = ident.sym.to_string();

        // Check if it's a tracked variable (from object access)
        if let Some(source) = self.variable_bindings.get(&name) {
            return source.clone();
        }

        // Check if it's an iterator parameter (search from innermost to outermost scope)
        if let Some(binding) = self.get_iterator_binding(&name) {
            return match &binding.binding_type {
                IteratorBindingType::StringArrayDirect => {
                    // Directly using the iterator var: KEYS.map(k => t(k))
                    self.resolve_string_array(&binding.array_name)
                }
                IteratorBindingType::ObjectArrayProperty => {
                    // Will be resolved when accessing property
                    // For now, return unknown variable
                    ValueSource::Unresolvable {
                        reason: UnresolvableReason::UnknownVariable(name),
                    }
                }
            };
        }

        // Unknown variable
        ValueSource::Unresolvable {
            reason: UnresolvableReason::UnknownVariable(name),
        }
    }

    /// Analyze a member expression (e.g., `obj.prop` or `item.key`).
    fn analyze_member(&self, member: &MemberExpr) -> ValueSource {
        // Check for iterator property access: cap.titleKey
        if let Expr::Ident(obj_ident) = &*member.obj
            && let MemberProp::Ident(prop_ident) = &member.prop
        {
            let obj_name = obj_ident.sym.to_string();
            let prop_name = prop_ident.sym.to_string();

            // Is this an iterator accessing a property? (search from innermost scope)
            if let Some(binding) = self.get_iterator_binding(&obj_name) {
                return self.resolve_array_property(&binding.array_name, &prop_name);
            }
        }

        ValueSource::Unresolvable {
            reason: UnresolvableReason::UnsupportedExpression {
                expr_type: "Member (unresolved)".to_string(),
            },
        }
    }

    // ============================================================
    // Registry lookup methods
    // ============================================================

    /// Resolve an object from the registry.
    pub fn resolve_object(&self, name: &str) -> ValueSource {
        // Check local file first
        let local_key = make_registry_key(self.file_path, name);
        if let Some(obj) = self.key_object_registry.get(&local_key) {
            return ValueSource::ObjectAccess {
                object_name: name.to_string(),
                candidate_values: obj.candidate_keys.clone(),
            };
        }

        // Check imports (only match if the target is exported and module-level)
        if let Some(resolved) = self.resolve_import(name, |key| {
            self.key_object_registry
                .get(key)
                .filter(|obj| obj.is_exported && obj.is_module_level)
        }) {
            return ValueSource::ObjectAccess {
                object_name: name.to_string(),
                candidate_values: resolved.candidate_keys.clone(),
            };
        }

        ValueSource::Unresolvable {
            reason: UnresolvableReason::UnknownObject(name.to_string()),
        }
    }

    /// Resolve a string array from the registry.
    pub fn resolve_string_array(&self, name: &str) -> ValueSource {
        // Check local file first
        let local_key = make_registry_key(self.file_path, name);
        if let Some(arr) = self.string_array_registry.get(&local_key) {
            return ValueSource::StringArrayElement {
                array_name: name.to_string(),
                candidate_values: arr.values.clone(),
            };
        }

        // Check imports (only match if the target is exported and module-level)
        if let Some(resolved) = self.resolve_import(name, |key| {
            self.string_array_registry
                .get(key)
                .filter(|arr| arr.is_exported && arr.is_module_level)
        }) {
            return ValueSource::StringArrayElement {
                array_name: name.to_string(),
                candidate_values: resolved.values.clone(),
            };
        }

        ValueSource::Unresolvable {
            reason: UnresolvableReason::UnknownArray(name.to_string()),
        }
    }

    /// Resolve an array property from the registry.
    pub fn resolve_array_property(&self, array_name: &str, prop_name: &str) -> ValueSource {
        // Check local file first
        let local_key = make_registry_key(self.file_path, array_name);
        if let Some(arr) = self.key_array_registry.get(&local_key)
            && let Some(values) = arr.property_values.get(prop_name)
        {
            return ValueSource::ArrayIteration {
                array_name: array_name.to_string(),
                property_name: prop_name.to_string(),
                candidate_values: values.clone(),
            };
        }

        // Check imports (only match if the target is exported and module-level)
        if let Some(resolved) = self.resolve_import(array_name, |key| {
            self.key_array_registry
                .get(key)
                .filter(|arr| arr.is_exported && arr.is_module_level)
        }) && let Some(values) = resolved.property_values.get(prop_name)
        {
            return ValueSource::ArrayIteration {
                array_name: array_name.to_string(),
                property_name: prop_name.to_string(),
                candidate_values: values.clone(),
            };
        }

        ValueSource::Unresolvable {
            reason: UnresolvableReason::UnknownArray(array_name.to_string()),
        }
    }

    /// Helper to resolve an import to a registry item.
    fn resolve_import<T, F>(&self, local_name: &str, lookup: F) -> Option<&'a T>
    where
        F: Fn(&str) -> Option<&'a T>,
    {
        // Find the import info for this local name
        let import_info = self
            .file_imports
            .iter()
            .find(|i| i.local_name == local_name)?;

        // Resolve the import path
        let resolved_path =
            resolve_import_path(Path::new(self.file_path), &import_info.module_path)?;

        // Look up in the registry using the resolved path
        let key = make_registry_key(&resolved_path, &import_info.imported_name);
        lookup(&key)
    }

    // ============================================================
    // Binding registration methods (called from AST visitor)
    // ============================================================

    /// Register a variable as coming from an object access.
    ///
    /// e.g., `const key = toolKeys[name]` -> register "key" with ObjectAccess source
    pub fn register_object_access(&mut self, var_name: &str, object_name: &str) {
        let source = self.resolve_object(object_name);
        self.variable_bindings.insert(var_name.to_string(), source);
    }

    /// Register an iterator variable from an array iteration.
    ///
    /// e.g., `KEYS.map(k => ...)` -> register "k" as iterating over "KEYS"
    pub fn register_iterator(&mut self, param_name: &str, array_name: &str) {
        // Determine if it's a string array or object array
        let local_key = make_registry_key(self.file_path, array_name);

        let binding_type = if self.string_array_registry.contains_key(&local_key) {
            IteratorBindingType::StringArrayDirect
        } else if self.key_array_registry.contains_key(&local_key) {
            IteratorBindingType::ObjectArrayProperty
        } else {
            // Check imports
            if let Some(import_info) = self
                .file_imports
                .iter()
                .find(|i| i.local_name == array_name)
            {
                if let Some(resolved_path) =
                    resolve_import_path(Path::new(self.file_path), &import_info.module_path)
                {
                    let key = make_registry_key(&resolved_path, &import_info.imported_name);
                    if self.string_array_registry.contains_key(&key) {
                        IteratorBindingType::StringArrayDirect
                    } else {
                        IteratorBindingType::ObjectArrayProperty
                    }
                } else {
                    IteratorBindingType::ObjectArrayProperty
                }
            } else {
                IteratorBindingType::ObjectArrayProperty
            }
        };

        // Register in current (innermost) scope
        if let Some(current_scope) = self.iterator_scopes.last_mut() {
            current_scope.insert(
                param_name.to_string(),
                IteratorBinding {
                    array_name: array_name.to_string(),
                    binding_type,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkers::key_objects::{ImportInfo, KeyArray, KeyObject, StringArray};

    fn create_empty_analyzer<'a>(
        file_path: &'a str,
        object_registry: &'a KeyObjectRegistry,
        array_registry: &'a KeyArrayRegistry,
        string_array_registry: &'a StringArrayRegistry,
        imports: &'a FileImports,
    ) -> ValueAnalyzer<'a> {
        ValueAnalyzer::new(
            file_path,
            object_registry,
            array_registry,
            string_array_registry,
            imports,
        )
    }

    #[test]
    fn test_resolve_string_array_local() {
        let object_registry = KeyObjectRegistry::new();
        let array_registry = KeyArrayRegistry::new();
        let mut string_array_registry = StringArrayRegistry::new();
        let imports = FileImports::new();

        string_array_registry.insert(
            make_registry_key("test.tsx", "KEYS"),
            StringArray {
                name: "KEYS".to_string(),
                file_path: "test.tsx".to_string(),
                is_exported: false,
                is_module_level: true,
                values: vec!["a".to_string(), "b".to_string()],
            },
        );

        let analyzer = create_empty_analyzer(
            "test.tsx",
            &object_registry,
            &array_registry,
            &string_array_registry,
            &imports,
        );

        let result = analyzer.resolve_string_array("KEYS");
        assert!(matches!(
            result,
            ValueSource::StringArrayElement {
                array_name,
                candidate_values
            } if array_name == "KEYS" && candidate_values == vec!["a", "b"]
        ));
    }

    #[test]
    fn test_resolve_object_local() {
        let mut object_registry = KeyObjectRegistry::new();
        let array_registry = KeyArrayRegistry::new();
        let string_array_registry = StringArrayRegistry::new();
        let imports = FileImports::new();

        object_registry.insert(
            make_registry_key("test.tsx", "toolKeys"),
            KeyObject {
                name: "toolKeys".to_string(),
                file_path: "test.tsx".to_string(),
                is_exported: false,
                is_module_level: true,
                candidate_keys: vec!["keyA".to_string(), "keyB".to_string()],
            },
        );

        let analyzer = create_empty_analyzer(
            "test.tsx",
            &object_registry,
            &array_registry,
            &string_array_registry,
            &imports,
        );

        let result = analyzer.resolve_object("toolKeys");
        assert!(matches!(
            result,
            ValueSource::ObjectAccess {
                object_name,
                candidate_values
            } if object_name == "toolKeys" && candidate_values == vec!["keyA", "keyB"]
        ));
    }

    #[test]
    fn test_resolve_array_property_local() {
        let object_registry = KeyObjectRegistry::new();
        let mut array_registry = KeyArrayRegistry::new();
        let string_array_registry = StringArrayRegistry::new();
        let imports = FileImports::new();

        let mut property_values = HashMap::new();
        property_values.insert(
            "titleKey".to_string(),
            vec!["a".to_string(), "b".to_string()],
        );

        array_registry.insert(
            make_registry_key("test.tsx", "capabilities"),
            KeyArray {
                name: "capabilities".to_string(),
                file_path: "test.tsx".to_string(),
                is_exported: false,
                is_module_level: true,
                property_values,
            },
        );

        let analyzer = create_empty_analyzer(
            "test.tsx",
            &object_registry,
            &array_registry,
            &string_array_registry,
            &imports,
        );

        let result = analyzer.resolve_array_property("capabilities", "titleKey");
        assert!(matches!(
            result,
            ValueSource::ArrayIteration {
                array_name,
                property_name,
                candidate_values
            } if array_name == "capabilities"
                && property_name == "titleKey"
                && candidate_values == vec!["a", "b"]
        ));
    }

    #[test]
    fn test_unknown_variable_returns_unresolvable() {
        let object_registry = KeyObjectRegistry::new();
        let array_registry = KeyArrayRegistry::new();
        let string_array_registry = StringArrayRegistry::new();
        let imports = FileImports::new();

        let analyzer = create_empty_analyzer(
            "test.tsx",
            &object_registry,
            &array_registry,
            &string_array_registry,
            &imports,
        );

        let result = analyzer.resolve_string_array("UNKNOWN");
        assert!(matches!(
            result,
            ValueSource::Unresolvable {
                reason: UnresolvableReason::UnknownArray(name)
            } if name == "UNKNOWN"
        ));
    }

    #[test]
    fn test_register_iterator_string_array() {
        let object_registry = KeyObjectRegistry::new();
        let array_registry = KeyArrayRegistry::new();
        let mut string_array_registry = StringArrayRegistry::new();
        let imports = FileImports::new();

        string_array_registry.insert(
            make_registry_key("test.tsx", "KEYS"),
            StringArray {
                name: "KEYS".to_string(),
                file_path: "test.tsx".to_string(),
                is_exported: false,
                is_module_level: true,
                values: vec!["a".to_string(), "b".to_string()],
            },
        );

        let mut analyzer = create_empty_analyzer(
            "test.tsx",
            &object_registry,
            &array_registry,
            &string_array_registry,
            &imports,
        );

        analyzer.register_iterator("k", "KEYS");

        // After registration, k should be recognized as iterating over KEYS
        assert!(analyzer.get_iterator_binding("k").is_some());
        assert!(matches!(
            analyzer.get_iterator_binding("k").unwrap().binding_type,
            IteratorBindingType::StringArrayDirect
        ));
    }

    // ============================================
    // Cross-file import resolution tests
    // ============================================
    // Note: Full cross-file import resolution with is_exported checking
    // cannot be tested in unit tests because resolve_import_path() checks
    // if files exist on disk. These scenarios are tested via CLI integration
    // tests in tests/cli/check.rs instead.

    #[test]
    fn test_import_without_matching_file_returns_unresolvable() {
        // When an import's target file doesn't exist/can't be resolved,
        // the lookup should return Unresolvable
        let mut object_registry = KeyObjectRegistry::new();
        object_registry.insert(
            make_registry_key("constants.ts", "toolKeys"),
            KeyObject {
                name: "toolKeys".to_string(),
                file_path: "constants.ts".to_string(),
                is_exported: true,
                is_module_level: true,
                candidate_keys: vec!["create".to_string()],
            },
        );

        let imports = vec![ImportInfo {
            local_name: "toolKeys".to_string(),
            imported_name: "toolKeys".to_string(),
            module_path: "./constants".to_string(), // File doesn't exist in tests
        }];

        let array_registry = KeyArrayRegistry::new();
        let string_array_registry = StringArrayRegistry::new();

        let analyzer = create_empty_analyzer(
            "component.tsx",
            &object_registry,
            &array_registry,
            &string_array_registry,
            &imports,
        );

        // Since the import can't be resolved (file doesn't exist),
        // this should return Unresolvable
        let result = analyzer.resolve_object("toolKeys");
        assert!(matches!(
            result,
            ValueSource::Unresolvable {
                reason: UnresolvableReason::UnknownObject(name)
            } if name == "toolKeys"
        ));
    }
}

//! Missing key checker implementation.
//!
//! Contains the `MissingKeyChecker` struct and its `Visit` trait implementation
//! for detecting translation keys used in code.

use std::collections::{HashMap, HashSet};

use swc_common::{Loc, SourceMap};
use swc_ecma_ast::{
    CallExpr, Callee, CondExpr, Expr, JSXElement, JSXFragment, Lit, MemberProp, Module, Pat,
    VarDecl,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::types::{
    DynamicKeyReason, DynamicKeyWarning, GlotAnnotation, MissingKeyResult, UsedKey,
};
use crate::checkers::{
    glob_matcher::{expand_glob_pattern, is_glob_pattern},
    key_objects::FileImports,
    schema::SchemaCallInfo,
    value_analyzer::ValueAnalyzer,
    value_source::ResolvedKey,
};
use crate::commands::context::Registries;
use crate::parsers::comment::{PatternWarning, extract_glot_message_keys};

const TRANSLATION_HOOKS: &[&str] = &["useTranslations", "getTranslations"];

fn is_translation_hook(name: &str) -> bool {
    TRANSLATION_HOOKS.contains(&name)
}

fn extract_namespace_from_call(call: &CallExpr) -> Option<String> {
    call.args.first().and_then(|arg| {
        if let Expr::Lit(Lit::Str(s)) = &*arg.expr {
            s.value.as_str().map(|s| s.to_string())
        } else {
            None
        }
    })
}

pub(crate) fn resolve_full_key(namespace: &Option<String>, key: &str) -> String {
    match namespace {
        Some(ns) => format!("{}.{}", ns, key),
        _ => key.to_string(),
    }
}

/// Checker for detecting missing translation keys.
///
/// Analyzes TSX/JSX files to find translation keys that are used in code
/// but may be missing from the locale JSON files.
pub struct MissingKeyChecker<'a> {
    file_path: &'a str,
    source_map: &'a SourceMap,
    bindings: HashMap<String, Option<String>>,
    pub used_keys: Vec<UsedKey>,
    pub warnings: Vec<DynamicKeyWarning>,
    registries: &'a Registries,
    pub schema_calls: Vec<SchemaCallInfo>,
    /// Unified value analyzer
    value_analyzer: ValueAnalyzer<'a>,
    /// Resolved keys from ValueAnalyzer
    pub resolved_keys: Vec<ResolvedKey>,
    /// glot-message-keys annotations: line -> (expanded keys, raw patterns for namespace prefixing)
    glot_annotations: HashMap<usize, GlotAnnotation>,
    /// Warnings from glot-message-keys annotation parsing
    pub pattern_warnings: Vec<PatternWarning>,
    /// Track whether we're currently inside JSX context
    in_jsx_context: bool,
}

impl<'a> MissingKeyChecker<'a> {
    pub fn new(
        file_path: &'a str,
        source_map: &'a SourceMap,
        registries: &'a Registries,
        file_imports: &'a FileImports,
        source: &str,
        available_keys: &HashSet<String>,
    ) -> Self {
        // Parse glot-message-keys annotations
        let (glot_annotations, pattern_warnings) =
            Self::parse_glot_annotations(source, file_path, available_keys);

        Self {
            file_path,
            source_map,
            bindings: HashMap::new(),
            used_keys: Vec::new(),
            warnings: Vec::new(),
            registries,
            schema_calls: Vec::new(),
            value_analyzer: ValueAnalyzer::new(
                file_path,
                &registries.key_object,
                &registries.key_array,
                &registries.string_array,
                file_imports,
            ),
            resolved_keys: Vec::new(),
            glot_annotations,
            pattern_warnings,
            in_jsx_context: false,
        }
    }

    /// Parse glot-message-keys annotations and build line -> keys mapping
    fn parse_glot_annotations(
        source: &str,
        file_path: &str,
        available_keys: &HashSet<String>,
    ) -> (HashMap<usize, GlotAnnotation>, Vec<PatternWarning>) {
        let mut annotations = HashMap::new();
        let extract_result = extract_glot_message_keys(source, file_path);

        for annotation in extract_result.annotations {
            let mut expanded_keys = Vec::new();

            for pattern in &annotation.patterns {
                if is_glob_pattern(pattern) {
                    // Expand glob pattern against available keys
                    let expanded = expand_glob_pattern(pattern, available_keys);
                    expanded_keys.extend(expanded);
                } else {
                    // Literal pattern - add as-is
                    expanded_keys.push(pattern.clone());
                }
            }

            if !expanded_keys.is_empty() {
                annotations.insert(
                    annotation.line,
                    GlotAnnotation {
                        keys: expanded_keys,
                    },
                );
            }
        }

        (annotations, extract_result.warnings)
    }

    /// Check if a line has glot-message-keys coverage.
    /// Returns the annotated keys if found on current line or previous line.
    fn get_glot_annotation(&self, line: usize) -> Option<&GlotAnnotation> {
        // Check current line first
        if let Some(ann) = self.glot_annotations.get(&line) {
            return Some(ann);
        }
        // Check previous line (annotation on the line before the t() call)
        if line > 1
            && let Some(ann) = self.glot_annotations.get(&(line - 1))
        {
            return Some(ann);
        }
        None
    }

    fn add_used_key(&mut self, loc: Loc, full_key: String) {
        let source_line = loc
            .file
            .get_line(loc.line - 1)
            .map(|cow| cow.to_string())
            .unwrap_or_default();
        self.used_keys.push(UsedKey {
            full_key,
            file_path: self.file_path.to_string(),
            line: loc.line,
            col: loc.col_display + 1,
            source_line,
        });
    }

    fn add_warning(&mut self, loc: Loc, reason: DynamicKeyReason, hint: Option<String>) {
        let source_line = loc
            .file
            .get_line(loc.line - 1)
            .map(|cow| cow.to_string())
            .unwrap_or_default();
        self.warnings.push(DynamicKeyWarning {
            file_path: self.file_path.to_string(),
            line: loc.line,
            col: loc.col_display + 1,
            reason,
            source_line,
            hint,
        });
    }

    /// Process a ternary expression: t(cond ? "keyA" : "keyB")
    /// If both branches are string literals, extract both keys.
    /// Otherwise, emit a dynamic key warning with appropriate reason.
    /// If `is_resolvable` is true, skip warnings (new ValueAnalyzer can handle it)
    fn process_ternary_arg(
        &mut self,
        cond: &CondExpr,
        loc: Loc,
        namespace: &Option<String>,
        is_resolvable: bool,
    ) {
        let cons_key = Self::extract_string_key(&cond.cons);
        let alt_key = Self::extract_string_key(&cond.alt);

        match (cons_key, alt_key) {
            (Some(k1), Some(k2)) => {
                // Both branches are string literals - add both as used keys
                let full_key1 = resolve_full_key(namespace, &k1);
                let full_key2 = resolve_full_key(namespace, &k2);
                self.add_used_key(loc.clone(), full_key1);
                self.add_used_key(loc, full_key2);
            }
            _ if !is_resolvable => {
                // At least one branch is dynamic and new ValueAnalyzer can't resolve
                let reason = if Self::is_template_with_expr(&cond.cons)
                    || Self::is_template_with_expr(&cond.alt)
                {
                    DynamicKeyReason::TemplateWithExpr
                } else {
                    DynamicKeyReason::VariableKey
                };
                // No hint for ternary - pattern inference is complex
                self.add_warning(loc, reason, None);
            }
            _ => {
                // New ValueAnalyzer can resolve, skip legacy warning
            }
        }
    }

    /// Check if an expression is a template literal with expressions.
    fn is_template_with_expr(expr: &Expr) -> bool {
        matches!(crate::checkers::unwrap_paren(expr), Expr::Tpl(tpl) if !tpl.exprs.is_empty())
    }

    /// Infer a glob pattern from a template literal.
    /// Returns None if the pattern would be too broad (e.g., "*" or "*.*").
    fn infer_pattern_from_template(
        tpl: &swc_ecma_ast::Tpl,
        namespace: &Option<String>,
    ) -> Option<String> {
        // Build pattern by joining quasis with "*"
        // e.g., `genderOptions.${x}` -> ["genderOptions.", ""] -> "genderOptions.*"
        // e.g., `step${x}.title` -> ["step", ".title"] -> "step*.title"
        let mut pattern = String::new();
        for (i, quasi) in tpl.quasis.iter().enumerate() {
            if let Some(cooked) = &quasi.cooked
                && let Some(s) = cooked.as_str()
            {
                pattern.push_str(s);
            }
            // Add * between quasis (not after the last one)
            if i < tpl.quasis.len() - 1 {
                pattern.push('*');
            }
        }

        // Apply namespace prefix if present
        let full_pattern = match namespace {
            Some(ns) => format!("{}.{}", ns, pattern),
            None => pattern,
        };

        // Check if pattern is too broad
        // Patterns like "*", "*.*", "*.*.*" are too broad
        let segments: Vec<&str> = full_pattern.split('.').collect();

        // Reject prefix wildcard patterns like "*.title" (same as validate_pattern)
        if segments.first().is_some_and(|s| *s == "*") && segments.len() > 1 {
            return None;
        }

        let all_wildcards = segments.iter().all(|s| *s == "*");
        if all_wildcards {
            return None;
        }

        // Pattern with more than 2 wildcards might be too broad
        let wildcard_count = full_pattern.matches('*').count();
        if wildcard_count > 2 {
            return None;
        }

        Some(full_pattern)
    }

    /// Extract a string key from an expression if it's a string literal or simple template.
    /// Unwraps parenthesized expressions first.
    fn extract_string_key(expr: &Expr) -> Option<String> {
        match crate::checkers::unwrap_paren(expr) {
            Expr::Lit(Lit::Str(s)) => s.value.as_str().map(|s| s.to_string()),
            Expr::Tpl(tpl) if tpl.exprs.is_empty() => tpl
                .quasis
                .first()
                .and_then(|q| q.cooked.as_ref())
                .and_then(|s| s.as_str())
                .map(|s| s.to_string()),
            _ => None,
        }
    }

    /// Extract object name from dynamic property access: `obj[key]`
    ///
    /// Only tracks computed/dynamic access because:
    /// - `obj[varName]` → key is dynamic, needs runtime resolution
    /// - `obj.prop` → key is static ("prop"), no tracking needed
    fn extract_object_access_name(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Member(member) if member.prop.is_computed() => match &*member.obj {
                Expr::Ident(obj_ident) => Some(obj_ident.sym.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Extract first parameter name from arrow function
    fn extract_arrow_first_param(expr: &Expr) -> Option<String> {
        if let Expr::Arrow(arrow) = expr
            && let Some(first_param) = arrow.params.first()
            && let Pat::Ident(ident) = first_param
        {
            return Some(ident.id.sym.to_string());
        }
        None
    }

    pub fn check(mut self, module: &Module) -> MissingKeyResult {
        self.visit_module(module);
        MissingKeyResult {
            used_keys: self.used_keys,
            warnings: self.warnings,
            schema_calls: self.schema_calls,
            resolved_keys: self.resolved_keys,
            pattern_warnings: self.pattern_warnings,
        }
    }
}

impl<'a> Visit for MissingKeyChecker<'a> {
    fn visit_jsx_element(&mut self, node: &JSXElement) {
        // Visit opening element (attributes) with in_jsx_context = false
        node.opening.visit_with(self);

        // Visit children with in_jsx_context = true
        let prev = self.in_jsx_context;
        self.in_jsx_context = true;
        for child in &node.children {
            child.visit_with(self);
        }
        self.in_jsx_context = prev;

        // Visit closing element (if any)
        if let Some(closing) = &node.closing {
            closing.visit_with(self);
        }
    }

    fn visit_jsx_fragment(&mut self, node: &JSXFragment) {
        // Visit opening
        node.opening.visit_with(self);

        // Visit children with in_jsx_context = true
        let prev = self.in_jsx_context;
        self.in_jsx_context = true;
        for child in &node.children {
            child.visit_with(self);
        }
        self.in_jsx_context = prev;

        // Visit closing
        node.closing.visit_with(self);
    }

    fn visit_var_decl(&mut self, node: &VarDecl) {
        for decl in &node.decls {
            if let Some(init) = &decl.init {
                let call_expr = match &**init {
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
                    if is_translation_hook(fn_name)
                        && let Pat::Ident(binding_ident) = &decl.name
                    {
                        let var_name = binding_ident.id.sym.to_string();
                        let namespace = extract_namespace_from_call(call);
                        self.bindings.insert(var_name, namespace);
                    }
                }

                // Track object access patterns: const key = obj[name]
                if let Some(object_name) = Self::extract_object_access_name(init)
                    && let Pat::Ident(binding_ident) = &decl.name
                {
                    let var_name = binding_ident.id.sym.to_string();
                    self.value_analyzer
                        .register_object_access(&var_name, &object_name);
                }
            }
        }
        node.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, node: &CallExpr) {
        if let Callee::Expr(expr) = &node.callee
            && let Expr::Ident(ident) = &**expr
        {
            let fn_name = ident.sym.as_str();

            if let Some(namespace) = self.bindings.get(fn_name).cloned() {
                let loc = self.source_map.lookup_char_pos(node.span.lo);

                if let Some(arg) = node.args.first() {
                    // Unified analysis with ValueAnalyzer
                    let source = self.value_analyzer.analyze_expr(&arg.expr);
                    let source_line = loc
                        .file
                        .get_line(loc.line - 1)
                        .map(|cow| cow.to_string())
                        .unwrap_or_default();

                    // Check if ValueAnalyzer can resolve the expression
                    let is_resolvable = source.resolve_keys().is_ok();

                    self.resolved_keys.push(ResolvedKey {
                        file_path: self.file_path.to_string(),
                        line: loc.line,
                        col: loc.col_display + 1,
                        source_line: source_line.clone(),
                        namespace: namespace.clone(),
                        source,
                    });

                    // Process static keys and emit warnings for unresolvable dynamic keys
                    match crate::checkers::unwrap_paren(&arg.expr) {
                        // Static string key: t("submit")
                        Expr::Lit(Lit::Str(s)) => {
                            if let Some(key) = s.value.as_str() {
                                let full_key = resolve_full_key(&namespace, key);
                                self.add_used_key(loc, full_key);
                            }
                        }
                        // Template literal without expressions: t(`submit`)
                        Expr::Tpl(tpl) if tpl.exprs.is_empty() => {
                            if let Some(quasi) = tpl.quasis.first()
                                && let Some(cooked) = &quasi.cooked
                                && let Some(key) = cooked.as_str()
                            {
                                let full_key = resolve_full_key(&namespace, key);
                                self.add_used_key(loc, full_key);
                            }
                        }
                        // Ternary with string literal branches: t(cond ? "keyA" : "keyB")
                        Expr::Cond(cond) => {
                            self.process_ternary_arg(cond, loc, &namespace, is_resolvable);
                        }
                        // Dynamic keys: check for glot-message-keys annotation first
                        _ if !is_resolvable => {
                            // Check for glot-message-keys annotation on current or previous line
                            // Clone the keys to avoid borrow checker issues
                            let annotated_keys = self
                                .get_glot_annotation(loc.line)
                                .map(|ann| ann.keys.clone());

                            if let Some(keys) = annotated_keys {
                                // Use annotated keys as used_keys (keys must be fully-qualified)
                                for key in keys {
                                    self.add_used_key(loc.clone(), key);
                                }
                            } else {
                                // No annotation - emit warning with hint if applicable
                                let unwrapped = crate::checkers::unwrap_paren(&arg.expr);
                                let (reason, hint) = if let Expr::Tpl(tpl) = unwrapped {
                                    let pattern =
                                        Self::infer_pattern_from_template(tpl, &namespace);
                                    let hint = pattern.map(|p| {
                                        if self.in_jsx_context {
                                            format!(
                                                "add `{{/* glot-message-keys \"{}\" */}}` to declare expected keys",
                                                p
                                            )
                                        } else {
                                            format!(
                                                "add `// glot-message-keys \"{}\"` to declare expected keys",
                                                p
                                            )
                                        }
                                    });
                                    (DynamicKeyReason::TemplateWithExpr, hint)
                                } else {
                                    (DynamicKeyReason::VariableKey, None)
                                };
                                self.add_warning(loc, reason, hint);
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Detect schema function calls
            if self.registries.schema.contains_key(fn_name)
                && let Some(arg) = node.args.first()
            {
                let should_record = match &*arg.expr {
                    Expr::Ident(t_ident) => {
                        let t_var = t_ident.sym.as_str();
                        self.bindings.contains_key(t_var)
                    }
                    _ => true,
                };

                if should_record {
                    let namespace = match &*arg.expr {
                        Expr::Ident(t_ident) => {
                            let t_var = t_ident.sym.as_str();
                            self.bindings.get(t_var).cloned().flatten()
                        }
                        _ => None,
                    };
                    let loc = self.source_map.lookup_char_pos(node.span.lo);

                    self.schema_calls.push(SchemaCallInfo {
                        schema_name: fn_name.to_string(),
                        namespace,
                        line: loc.line,
                        col: loc.col_display + 1,
                    });
                }
            }
        }

        // Handle method calls on translation function: t.raw("key"), t.rich("key"), t.markup("key")
        // These are MemberExpr calls where obj is the translation function variable
        if let Callee::Expr(expr) = &node.callee
            && let Expr::Member(member) = &**expr
            && let Expr::Ident(obj_ident) = &*member.obj
            && let MemberProp::Ident(method_ident) = &member.prop
        {
            let obj_name = obj_ident.sym.as_str();
            let method_name = method_ident.sym.as_str();

            // Check if this is a translation method call (t.raw, t.rich, t.markup)
            if matches!(method_name, "raw" | "rich" | "markup")
                && let Some(namespace) = self.bindings.get(obj_name).cloned()
                && let Some(arg) = node.args.first()
            {
                let loc = self.source_map.lookup_char_pos(node.span.lo);

                // For t.raw/t.rich/t.markup, extract the key from first argument
                match crate::checkers::unwrap_paren(&arg.expr) {
                    // Static string key: t.raw("benefits")
                    Expr::Lit(Lit::Str(s)) => {
                        if let Some(key) = s.value.as_str() {
                            let full_key = resolve_full_key(&namespace, key);
                            self.add_used_key(loc, full_key);
                        }
                    }
                    // Template literal without expressions: t.raw(`benefits`)
                    Expr::Tpl(tpl) if tpl.exprs.is_empty() => {
                        if let Some(quasi) = tpl.quasis.first()
                            && let Some(cooked) = &quasi.cooked
                            && let Some(key) = cooked.as_str()
                        {
                            let full_key = resolve_full_key(&namespace, key);
                            self.add_used_key(loc, full_key);
                        }
                    }
                    _ => {
                        // Dynamic keys in t.raw() etc are not common, skip for now
                    }
                }
            }
        }

        // Detect iterator patterns: array.map(item => ...) or array.forEach(item => ...)
        // Enter a new scope for the callback to properly handle nested iterators
        let entered_scope = if let Callee::Expr(callee_expr) = &node.callee
            && let Expr::Member(member) = &**callee_expr
            && let MemberProp::Ident(method) = &member.prop
            && matches!(
                method.sym.as_str(),
                "map" | "forEach" | "filter" | "find" | "some" | "every" | "flatMap"
            )
            && let Expr::Ident(array_ident) = &*member.obj
            && let Some(arg) = node.args.first()
            && let Some(param_name) = Self::extract_arrow_first_param(&arg.expr)
        {
            let array_name = array_ident.sym.to_string();
            self.value_analyzer.enter_scope();
            self.value_analyzer
                .register_iterator(&param_name, &array_name);
            true
        } else {
            false
        };

        node.visit_children_with(self);

        // Exit scope after visiting the callback body
        if entered_scope {
            self.value_analyzer.exit_scope();
        }
    }
}

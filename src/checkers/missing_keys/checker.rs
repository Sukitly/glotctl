//! Missing key checker implementation.
//!
//! Contains the `MissingKeyChecker` struct and its `Visit` trait implementation
//! for detecting translation keys used in code.

use std::collections::{HashMap, HashSet};

use swc_common::{Loc, SourceMap};
use swc_ecma_ast::{
    CallExpr, Callee, CondExpr, Expr, FnDecl, JSXElement, JSXFragment, Lit, MemberProp, Module,
    ObjectPatProp, Pat, VarDecl, VarDeclarator,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::types::{
    DynamicKeyReason, DynamicKeyWarning, GlotAnnotation, MissingKeyResult, TranslationSource,
    UsedKey,
};
use crate::checkers::{
    glob_matcher::{expand_glob_pattern, is_glob_pattern},
    key_objects::{FileImports, make_translation_fn_call_key, make_translation_prop_key},
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
    /// Stack of translation function bindings scoped by function/arrow.
    /// e.g., { "t" -> Direct { namespace: Some("Common") } }
    bindings_stack: Vec<HashMap<String, TranslationSource>>,
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
    /// Available keys for glob expansion (stored for relative pattern expansion)
    available_keys: &'a HashSet<String>,
}

impl<'a> MissingKeyChecker<'a> {
    pub fn new(
        file_path: &'a str,
        source_map: &'a SourceMap,
        registries: &'a Registries,
        file_imports: &'a FileImports,
        source: &str,
        available_keys: &'a HashSet<String>,
    ) -> Self {
        // Parse glot-message-keys annotations
        let (glot_annotations, pattern_warnings) =
            Self::parse_glot_annotations(source, file_path, available_keys);

        Self {
            file_path,
            source_map,
            bindings_stack: vec![HashMap::new()],
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
            available_keys,
        }
    }

    /// Parse glot-message-keys annotations and build line -> keys mapping.
    ///
    /// Handles two types of patterns:
    /// - Absolute patterns: `Namespace.key.path` - expanded immediately
    /// - Relative patterns: `.key.path` (starting with `.`) - stored for later expansion
    ///   with the namespace from the translation function binding
    fn parse_glot_annotations(
        source: &str,
        file_path: &str,
        available_keys: &HashSet<String>,
    ) -> (HashMap<usize, GlotAnnotation>, Vec<PatternWarning>) {
        let mut annotations = HashMap::new();
        let extract_result = extract_glot_message_keys(source, file_path);

        for annotation in extract_result.annotations {
            let mut expanded_keys = Vec::new();
            let mut relative_patterns = Vec::new();

            for pattern in &annotation.patterns {
                if pattern.starts_with('.') {
                    // Relative pattern - store for later expansion with namespace
                    relative_patterns.push(pattern.clone());
                } else if is_glob_pattern(pattern) {
                    // Absolute glob pattern - expand immediately
                    let expanded = expand_glob_pattern(pattern, available_keys);
                    expanded_keys.extend(expanded);
                } else {
                    // Absolute literal pattern - add as-is
                    expanded_keys.push(pattern.clone());
                }
            }

            if !expanded_keys.is_empty() || !relative_patterns.is_empty() {
                annotations.insert(
                    annotation.line,
                    GlotAnnotation {
                        keys: expanded_keys,
                        relative_patterns,
                    },
                );
            }
        }

        (annotations, extract_result.warnings)
    }

    fn enter_scope(&mut self) {
        self.bindings_stack.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.bindings_stack.pop();
    }

    fn insert_binding(&mut self, name: String, source: TranslationSource) {
        if let Some(scope) = self.bindings_stack.last_mut() {
            scope.insert(name, source);
        }
    }

    fn get_binding(&self, name: &str) -> Option<TranslationSource> {
        for scope in self.bindings_stack.iter().rev() {
            if let Some(source) = scope.get(name) {
                return Some(source.clone());
            }
        }
        None
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

    /// Add used keys for a relative key with all namespaces from the translation source.
    /// For Direct sources, adds a single key.
    /// For FromProps sources, adds a key for each possible namespace.
    fn add_used_keys_with_namespaces(
        &mut self,
        loc: Loc,
        key: &str,
        translation_source: &TranslationSource,
    ) {
        let namespaces = translation_source.namespaces();
        for namespace in namespaces {
            let full_key = resolve_full_key(&namespace, key);
            self.add_used_key(loc.clone(), full_key);
        }
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

    /// Expand relative patterns (starting with `.`) with namespace(s).
    ///
    /// For Direct sources: expands with the single namespace.
    /// For FromProps sources: expands with all possible namespaces.
    ///
    /// Relative patterns like `.features.*.title` become `Namespace.features.*.title`.
    /// Glob patterns within relative patterns are expanded against available keys.
    /// If a glob pattern doesn't match any keys, the original pattern is preserved
    /// (to allow missing-key detection to report it).
    fn expand_relative_patterns(
        &self,
        relative_patterns: &[String],
        translation_source: &TranslationSource,
    ) -> Vec<String> {
        if relative_patterns.is_empty() {
            return Vec::new();
        }

        let namespaces = translation_source.namespaces();
        let mut result = Vec::new();

        for pattern in relative_patterns {
            // Remove the leading `.` to get the relative path
            let relative_path = &pattern[1..];

            for namespace in &namespaces {
                let full_pattern = match namespace {
                    Some(ns) => format!("{}.{}", ns, relative_path),
                    None => relative_path.to_string(),
                };

                // If the full pattern contains glob, expand it against available keys
                if is_glob_pattern(&full_pattern) {
                    let expanded = expand_glob_pattern(&full_pattern, self.available_keys);
                    if expanded.is_empty() {
                        // No matches - keep the pattern for missing-key reporting
                        result.push(full_pattern);
                    } else {
                        result.extend(expanded);
                    }
                } else {
                    result.push(full_pattern);
                }
            }
        }

        result
    }

    /// Process a ternary expression: t(cond ? "keyA" : "keyB")
    /// If both branches are string literals, extract both keys.
    /// Otherwise, emit a dynamic key warning with appropriate reason.
    /// If `is_resolvable` is true, skip warnings (new ValueAnalyzer can handle it)
    fn process_ternary_arg(
        &mut self,
        cond: &CondExpr,
        loc: Loc,
        translation_source: &TranslationSource,
        is_resolvable: bool,
    ) {
        let cons_key = Self::extract_string_key(&cond.cons);
        let alt_key = Self::extract_string_key(&cond.alt);

        match (cons_key, alt_key) {
            (Some(k1), Some(k2)) => {
                // Both branches are string literals - add both as used keys
                self.add_used_keys_with_namespaces(loc.clone(), &k1, translation_source);
                self.add_used_keys_with_namespaces(loc, &k2, translation_source);
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

    /// Register translation function bindings from function parameters.
    ///
    /// Checks if the component exists in the TranslationPropRegistry and
    /// registers any matching prop names as FromProps bindings.
    fn register_translation_props_from_params(
        &mut self,
        component_name: &str,
        params: &[swc_ecma_ast::Param],
    ) {
        // Get the first parameter - typically the props object in React components
        let Some(first_param) = params.first() else {
            return;
        };

        self.register_translation_props_from_pat(component_name, &first_param.pat);
    }

    /// Register translation props from a pattern (handles destructuring).
    fn register_translation_props_from_pat(&mut self, component_name: &str, pat: &Pat) {
        match pat {
            // Destructured props: function Component({ t, features }) { ... }
            Pat::Object(obj_pat) => {
                for prop in &obj_pat.props {
                    if let ObjectPatProp::KeyValue(kv) = prop
                        && let swc_ecma_ast::PropName::Ident(key_ident) = &kv.key
                    {
                        // Renamed destructuring: { t: translate }
                        // - prop_name: "t" (original prop name, for registry lookup)
                        // - binding_name: "translate" (actual variable name in code)
                        let prop_name = key_ident.sym.to_string();
                        let binding_name = Self::extract_binding_name_from_pat(&kv.value);
                        if let Some(binding) = binding_name {
                            self.try_register_translation_prop(
                                component_name,
                                &prop_name,
                                &binding,
                            );
                        }
                    } else if let ObjectPatProp::Assign(assign) = prop {
                        // Shorthand destructuring: { t } or { t = defaultValue }
                        // prop_name and binding_name are the same
                        let prop_name = assign.key.sym.to_string();
                        self.try_register_translation_prop(component_name, &prop_name, &prop_name);
                    }
                }
            }
            // Non-destructured props: function Component(props) { ... }
            // We can't easily track `props.t` calls, so skip for now
            Pat::Ident(_) => {}
            _ => {}
        }
    }

    /// Extract the binding name from a pattern.
    /// For `{ t: translate }`, the value pattern is `Ident("translate")`.
    /// For `{ t: translate = defaultValue }`, the value pattern is `Assign` with left being `Ident("translate")`.
    fn extract_binding_name_from_pat(pat: &Pat) -> Option<String> {
        match pat {
            Pat::Ident(ident) => Some(ident.id.sym.to_string()),
            Pat::Assign(assign) => {
                // { t: translate = defaultValue }
                if let Pat::Ident(ident) = &*assign.left {
                    Some(ident.id.sym.to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Try to register a translation prop binding if it exists in the registry.
    ///
    /// - `prop_name`: The original prop name (e.g., "t"), used for registry lookup
    /// - `binding_name`: The actual variable name in code (e.g., "translate" for `{ t: translate }`)
    fn try_register_translation_prop(
        &mut self,
        component_name: &str,
        prop_name: &str,
        binding_name: &str,
    ) {
        let key = make_translation_prop_key(component_name, prop_name);

        if let Some(translation_prop) = self.registries.translation_prop.get(&key) {
            // Register this prop as a translation function binding with FromProps source
            self.insert_binding(
                binding_name.to_string(),
                TranslationSource::FromProps {
                    namespaces: translation_prop.namespaces.clone(),
                },
            );
        }
    }

    /// Register translation function bindings from function/arrow parameters.
    ///
    /// This handles the case where a translation function is passed as a regular
    /// function argument (not as a JSX prop). For example:
    ///
    /// ```tsx
    /// const usageLabels = (t) => ({ key: t("key") });
    /// const labels = usageLabels(t);  // t passed as function argument
    /// ```
    ///
    /// Register translation function parameters for a function.
    /// If the function is in the translation_fn_call registry, register parameters with FromFnCall.
    /// For any parameter that shadows an outer translation binding but isn't registered,
    /// insert a Shadowed marker to prevent the outer binding from leaking in.
    ///
    /// - `fn_name`: The function name (e.g., "usageLabels")
    /// - `params`: The function parameters
    fn register_translation_fn_params(&mut self, fn_name: &str, params: &[Pat]) {
        for (idx, param) in params.iter().enumerate() {
            if let Pat::Ident(ident) = param {
                let param_name = ident.id.sym.to_string();
                // Check if this function+param is in the registry
                let key = make_translation_fn_call_key(self.file_path, fn_name, idx);
                if let Some(fn_call) = self.registries.translation_fn_call.get(&key) {
                    // Register this parameter as a translation function binding
                    self.insert_binding(
                        param_name,
                        TranslationSource::FromFnCall {
                            namespaces: fn_call.namespaces.clone(),
                        },
                    );
                } else if self.get_binding(&param_name).is_some() {
                    // Not in registry, but there's an outer binding with this name.
                    // Shadow it to prevent incorrect tracking.
                    self.insert_binding(param_name, TranslationSource::Shadowed);
                }
            }
        }
    }

    /// Check if a variable declarator is an arrow function component.
    fn extract_arrow_component(decl: &VarDeclarator) -> Option<(String, &swc_ecma_ast::ArrowExpr)> {
        let Pat::Ident(binding_ident) = &decl.name else {
            return None;
        };
        let Some(init) = &decl.init else {
            return None;
        };
        let Expr::Arrow(arrow) = &**init else {
            return None;
        };

        let component_name = binding_ident.id.sym.to_string();
        Some((component_name, arrow))
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

    /// Visit function declarations to detect translation function parameters.
    /// e.g., `function AdultLandingPage({ t }: Props) { ... }`
    /// e.g., `function usageLabels(t) { ... }`
    fn visit_fn_decl(&mut self, node: &FnDecl) {
        self.enter_scope();
        let fn_name = node.ident.sym.to_string();

        // Try to register translation props (for React components)
        self.register_translation_props_from_params(&fn_name, &node.function.params);

        // Try to register translation function parameters (for utility functions)
        // Convert Param to Pat for the helper function
        let params: Vec<Pat> = node.function.params.iter().map(|p| p.pat.clone()).collect();
        self.register_translation_fn_params(&fn_name, &params);

        node.function.visit_children_with(self);
        self.exit_scope();
    }

    fn visit_function(&mut self, node: &swc_ecma_ast::Function) {
        self.enter_scope();
        node.visit_children_with(self);
        self.exit_scope();
    }

    fn visit_arrow_expr(&mut self, node: &swc_ecma_ast::ArrowExpr) {
        self.enter_scope();
        node.visit_children_with(self);
        self.exit_scope();
    }

    /// Visit variable declarations to detect:
    /// 1. Translation hook bindings: `const t = useTranslations("Ns")`
    /// 2. Arrow function components: `const MyComponent = ({ t }: Props) => { ... }`
    /// 3. Arrow functions with translation function parameters: `const fn = (t) => { ... }`
    fn visit_var_decl(&mut self, node: &VarDecl) {
        for decl in &node.decls {
            // Special handling for arrow functions (components or utility functions):
            // We need to manually manage scope here because:
            // 1. Arrow functions need to register translation bindings (from props or fn args)
            //    BEFORE visiting the function body
            // 2. Using `arrow.visit_children_with(self)` only visits the body/params,
            //    it won't trigger `visit_arrow_expr` again (avoiding double scoping)
            // 3. The `continue` skips the default `decl.visit_children_with(self)` to avoid
            //    processing the same arrow expression twice
            if let Some((fn_name, arrow)) = Self::extract_arrow_component(decl) {
                self.enter_scope();

                // Try to register translation props (for React components)
                if let Some(first_param) = arrow.params.first() {
                    self.register_translation_props_from_pat(&fn_name, first_param);
                }

                // Try to register translation function parameters (for utility functions)
                // This handles: const usageLabels = (t) => { t("key") }
                self.register_translation_fn_params(&fn_name, &arrow.params);

                arrow.visit_children_with(self);
                self.exit_scope();
                continue;
            }

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
                        self.insert_binding(var_name, TranslationSource::Direct { namespace });
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
            decl.visit_children_with(self);
        }
    }

    fn visit_call_expr(&mut self, node: &CallExpr) {
        if let Callee::Expr(expr) = &node.callee
            && let Expr::Ident(ident) = &**expr
        {
            let fn_name = ident.sym.as_str();

            if let Some(translation_source) = self.get_binding(fn_name)
                && !translation_source.is_shadowed()
            {
                let namespace = translation_source.primary_namespace();
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
                                self.add_used_keys_with_namespaces(loc, key, &translation_source);
                            }
                        }
                        // Template literal without expressions: t(`submit`)
                        Expr::Tpl(tpl) if tpl.exprs.is_empty() => {
                            if let Some(quasi) = tpl.quasis.first()
                                && let Some(cooked) = &quasi.cooked
                                && let Some(key) = cooked.as_str()
                            {
                                self.add_used_keys_with_namespaces(loc, key, &translation_source);
                            }
                        }
                        // Ternary with string literal branches: t(cond ? "keyA" : "keyB")
                        Expr::Cond(cond) => {
                            self.process_ternary_arg(cond, loc, &translation_source, is_resolvable);
                        }
                        // Dynamic keys: check for glot-message-keys annotation first
                        _ if !is_resolvable => {
                            // Check for glot-message-keys annotation on current or previous line
                            // Clone the data to avoid borrow checker issues
                            let annotation_data = self
                                .get_glot_annotation(loc.line)
                                .map(|ann| (ann.keys.clone(), ann.relative_patterns.clone()));

                            if let Some((keys, relative_patterns)) = annotation_data {
                                // Use absolute annotated keys as used_keys
                                for key in keys {
                                    self.add_used_key(loc.clone(), key);
                                }

                                // Expand relative patterns with namespace(s)
                                let expanded_relative = self.expand_relative_patterns(
                                    &relative_patterns,
                                    &translation_source,
                                );
                                for key in expanded_relative {
                                    self.add_used_key(loc.clone(), key);
                                }
                            } else {
                                // No annotation - emit warning with hint if applicable
                                let unwrapped = crate::checkers::unwrap_paren(&arg.expr);
                                let (reason, hint) = if let Expr::Tpl(tpl) = unwrapped {
                                    // For indirect sources (FromProps, FromFnCall), suggest relative pattern (starting with .)
                                    // For Direct, use absolute pattern with namespace
                                    let pattern = if translation_source.is_indirect() {
                                        Self::infer_pattern_from_template(tpl, &None)
                                            .map(|p| format!(".{}", p))
                                    } else {
                                        Self::infer_pattern_from_template(tpl, &namespace)
                                    };
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
                        self.get_binding(t_var)
                            .is_some_and(|src| !src.is_shadowed())
                    }
                    _ => true,
                };

                if should_record {
                    let namespace = match &*arg.expr {
                        Expr::Ident(t_ident) => {
                            let t_var = t_ident.sym.as_str();
                            self.get_binding(t_var)
                                .and_then(|src| src.primary_namespace())
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
                && let Some(translation_source) = self.get_binding(obj_name)
                && !translation_source.is_shadowed()
                && let Some(arg) = node.args.first()
            {
                let loc = self.source_map.lookup_char_pos(node.span.lo);

                // For t.raw/t.rich/t.markup, extract the key from first argument
                match crate::checkers::unwrap_paren(&arg.expr) {
                    // Static string key: t.raw("benefits")
                    Expr::Lit(Lit::Str(s)) => {
                        if let Some(key) = s.value.as_str() {
                            self.add_used_keys_with_namespaces(
                                loc.clone(),
                                key,
                                &translation_source,
                            );
                        }
                    }
                    // Template literal without expressions: t.raw(`benefits`)
                    Expr::Tpl(tpl) if tpl.exprs.is_empty() => {
                        if let Some(quasi) = tpl.quasis.first()
                            && let Some(cooked) = &quasi.cooked
                            && let Some(key) = cooked.as_str()
                        {
                            self.add_used_keys_with_namespaces(
                                loc.clone(),
                                key,
                                &translation_source,
                            );
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

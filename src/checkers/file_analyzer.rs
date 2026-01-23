//! Combined file analyzer that extracts translation keys and detects hardcoded text in a single AST traversal.
//!
//! This module merges the functionality of:
//! - `HardcodedChecker`: detects hardcoded text in JSX/TSX
//! - `TranslationKeyVisitor`: extracts translation keys from code
//! - `TranslationCallFinder`: finds lines with translation calls (derived from used_keys)
//!
//! By combining these into a single visitor, we reduce the number of AST traversals per file
//! from 3-4 to just 1, significantly improving performance.

use std::collections::HashSet;

use swc_common::{Loc, SourceMap, comments::SingleThreadedComments};
use swc_ecma_ast::{
    BinaryOp, CallExpr, Callee, CondExpr, DefaultDecl, Expr, FnDecl, JSXAttr, JSXAttrName,
    JSXAttrValue, JSXElement, JSXElementName, JSXExpr, JSXExprContainer, JSXFragment, JSXText,
    Lit, MemberProp, Module, ObjectPatProp, Pat, VarDecl, VarDeclarator,
};
use swc_ecma_visit::{Visit, VisitWith};

use crate::directives::{DisableContext, DisableRule};
use crate::issue::{HardcodedIssue, SourceLocation};
use crate::utils::contains_alphabetic;

use super::extraction::{
    DynamicKeyReason, DynamicKeyWarning, KeyExtractionResult, UsedKey,
    annotation_store::AnnotationStore,
    binding_context::BindingContext,
    translation_source::TranslationSource,
};
use super::{
    extract_namespace_from_call, is_translation_hook,
    key_objects::{FileImports, extract_binding_names, make_translation_fn_call_key},
    schema::SchemaCallInfo,
    value_analyzer::ValueAnalyzer,
    value_source::ResolvedKey,
};
use crate::commands::context::Registries;

/// Tracks JSX context state during AST traversal.
///
/// These flags are independent (not mutually exclusive):
/// - `in_context`: Inside JSX element/fragment children
/// - `in_attr`: Inside a JSX attribute
/// - `in_checked_attr`: Inside a checked attribute (placeholder, title, etc.)
/// - `in_expr`: Inside a JSX expression container {}
/// - `in_element_expr`: Current JSX element is inside an expression (ternary, &&, etc.)
#[derive(Debug, Clone, Copy, Default)]
struct JsxState {
    in_context: bool,
    in_attr: bool,
    in_checked_attr: bool,
    in_expr: bool,
    in_element_expr: bool,
}

impl JsxState {
    /// Create a new state for entering JSX children.
    /// Preserves `in_element_expr` based on whether we're currently in an expression.
    fn for_children(self) -> Self {
        Self {
            in_context: true,
            in_attr: false,
            in_checked_attr: false,
            in_expr: false,
            in_element_expr: self.in_expr, // if entering from expr, element is in expr
        }
    }
}

/// Result of analyzing a single file.
#[derive(Debug)]
pub struct FileAnalysisResult {
    pub hardcoded_issues: Vec<HardcodedIssue>,
    pub extraction: KeyExtractionResult,
}

/// Combined analyzer that extracts translation keys and detects hardcoded text.
pub struct FileAnalyzer<'a> {
    // === Shared fields ===
    file_path: &'a str,
    source_map: &'a SourceMap,
    disable_context: DisableContext,
    jsx_state: JsxState,

    // === HardcodedChecker specific ===
    checked_attributes: &'a [String],
    ignore_texts: &'a HashSet<String>,

    // === TranslationKeyVisitor specific ===
    binding_context: BindingContext,
    annotation_store: AnnotationStore,
    value_analyzer: ValueAnalyzer<'a>,
    registries: &'a Registries,
    available_keys: &'a HashSet<String>,

    // === Output ===
    hardcoded_issues: Vec<HardcodedIssue>,
    used_keys: Vec<UsedKey>,
    warnings: Vec<DynamicKeyWarning>,
    schema_calls: Vec<SchemaCallInfo>,
    resolved_keys: Vec<ResolvedKey>,
}

impl<'a> FileAnalyzer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file_path: &'a str,
        source_map: &'a SourceMap,
        comments: &SingleThreadedComments,
        checked_attributes: &'a [String],
        ignore_texts: &'a HashSet<String>,
        registries: &'a Registries,
        file_imports: &'a FileImports,
        source: &str,
        available_keys: &'a HashSet<String>,
    ) -> Self {
        let annotation_store = AnnotationStore::parse(source, file_path, available_keys);
        let disable_context = DisableContext::from_comments(comments, source_map);

        Self {
            file_path,
            source_map,
            disable_context,
            jsx_state: JsxState::default(),
            checked_attributes,
            ignore_texts,
            binding_context: BindingContext::new(),
            annotation_store,
            value_analyzer: ValueAnalyzer::new(
                file_path,
                &registries.key_object,
                &registries.key_array,
                &registries.string_array,
                file_imports,
            ),
            registries,
            available_keys,
            hardcoded_issues: Vec::new(),
            used_keys: Vec::new(),
            warnings: Vec::new(),
            schema_calls: Vec::new(),
            resolved_keys: Vec::new(),
        }
    }

    /// Main entry point: analyze a module and return results.
    pub fn analyze(mut self, module: &Module) -> FileAnalysisResult {
        self.visit_module(module);
        FileAnalysisResult {
            hardcoded_issues: self.hardcoded_issues,
            extraction: KeyExtractionResult {
                used_keys: self.used_keys,
                warnings: self.warnings,
                schema_calls: self.schema_calls,
                resolved_keys: self.resolved_keys,
                pattern_warnings: self.annotation_store.warnings,
            },
        }
    }

    // ============================================================
    // HardcodedChecker methods
    // ============================================================

    fn should_report_hardcoded(&self, line: usize, text: &str) -> bool {
        if self
            .disable_context
            .should_ignore(line, DisableRule::Hardcoded)
        {
            return false;
        }
        let text = text.trim();
        if self.ignore_texts.contains(text) {
            return false;
        }
        contains_alphabetic(text)
    }

    /// Determines whether to use JSX comment style `{/* */}` or JS comment style `//`.
    fn should_use_jsx_comment(&self, source_line: &str) -> bool {
        let trimmed_line = source_line.trim_start();
        if self.jsx_state.in_expr
            && (trimmed_line.starts_with(':') || trimmed_line.starts_with('?'))
        {
            return false;
        }
        let line_starts_with_element = trimmed_line.starts_with('<');
        let state = &self.jsx_state;

        if state.in_attr {
            line_starts_with_element && state.in_context && !state.in_expr
        } else if line_starts_with_element {
            if state.in_element_expr {
                false
            } else {
                state.in_context && !state.in_expr
            }
        } else {
            state.in_context
        }
    }

    fn add_hardcoded_issue(&mut self, value: &str, loc: Loc) {
        let source_line = loc
            .file
            .get_line(loc.line - 1)
            .map(|cow| cow.to_string())
            .unwrap_or_default();

        let use_jsx_comment = self.should_use_jsx_comment(&source_line);

        self.hardcoded_issues.push(HardcodedIssue {
            location: SourceLocation::new(self.file_path, loc.line)
                .with_col(loc.col_display + 1)
                .with_jsx_context(use_jsx_comment),
            text: value.to_owned(),
            source_line: Some(source_line),
        });
    }

    fn check_hardcoded_line(&mut self, value: &str, loc: Loc) {
        if self.should_report_hardcoded(loc.line, value) {
            self.add_hardcoded_issue(value, loc);
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Lit(Lit::Str(s)) => {
                if let Some(value) = s.value.as_str() {
                    let loc = self.source_map.lookup_char_pos(s.span.lo);
                    self.check_hardcoded_line(value, loc);
                };
            }
            Expr::Tpl(tpl) => {
                for quasi in &tpl.quasis {
                    if let Some(cooked) = &quasi.cooked
                        && let Some(value) = cooked.as_str()
                    {
                        let loc = self.source_map.lookup_char_pos(quasi.span.lo);
                        self.check_hardcoded_line(value, loc);
                    }
                }
            }
            Expr::Bin(bin) if bin.op == BinaryOp::LogicalAnd || bin.op == BinaryOp::LogicalOr => {
                self.check_expr(&bin.right)
            }
            Expr::Cond(cond) => {
                self.check_expr(&cond.cons);
                self.check_expr(&cond.alt);
            }
            _ => {}
        }
    }

    // ============================================================
    // TranslationKeyVisitor methods
    // ============================================================

    fn should_use_jsx_comment_for_extraction(&self, source_line: &str) -> bool {
        // Use JSX state for determining comment style
        if !self.jsx_state.in_context {
            return false;
        }
        if self.jsx_state.in_expr {
            let trimmed = source_line.trim_start();
            if trimmed.starts_with(':') || trimmed.starts_with('?') {
                return false;
            }
        }
        true
    }

    fn add_used_key(&mut self, loc: Loc, full_key: String) {
        let source_line = loc
            .file
            .get_line(loc.line - 1)
            .map(|cow| cow.to_string())
            .unwrap_or_default();
        let untranslated_disabled = self
            .disable_context
            .should_ignore(loc.line, DisableRule::Untranslated);
        let in_jsx_context = self.should_use_jsx_comment_for_extraction(&source_line);
        self.used_keys.push(UsedKey {
            full_key,
            file_path: self.file_path.to_string(),
            line: loc.line,
            col: loc.col_display + 1,
            source_line,
            in_jsx_context,
            untranslated_disabled,
        });
    }

    fn add_used_keys_with_namespaces(
        &mut self,
        loc: Loc,
        key: &str,
        translation_source: &TranslationSource,
    ) {
        let namespaces = translation_source.namespaces();
        for namespace in namespaces {
            let full_key = self.resolve_full_key(&namespace, key);
            self.add_used_key(loc.clone(), full_key);
        }
    }

    fn add_warning(
        &mut self,
        loc: Loc,
        reason: DynamicKeyReason,
        hint: Option<String>,
        pattern: Option<String>,
    ) {
        let source_line = loc
            .file
            .get_line(loc.line - 1)
            .map(|cow| cow.to_string())
            .unwrap_or_default();
        let in_jsx_context = self.should_use_jsx_comment_for_extraction(&source_line);
        self.warnings.push(DynamicKeyWarning {
            file_path: self.file_path.to_string(),
            line: loc.line,
            col: loc.col_display + 1,
            reason,
            source_line,
            hint,
            pattern,
            in_jsx_context,
        });
    }

    fn resolve_full_key(&self, namespace: &Option<String>, key: &str) -> String {
        match namespace {
            Some(ns) => format!("{}.{}", ns, key),
            _ => key.to_string(),
        }
    }

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
                self.add_used_keys_with_namespaces(loc.clone(), &k1, translation_source);
                self.add_used_keys_with_namespaces(loc, &k2, translation_source);
            }
            _ if !is_resolvable => {
                let reason = if Self::is_template_with_expr(&cond.cons)
                    || Self::is_template_with_expr(&cond.alt)
                {
                    DynamicKeyReason::TemplateWithExpr
                } else {
                    DynamicKeyReason::VariableKey
                };
                self.add_warning(loc, reason, None, None);
            }
            _ => {}
        }
    }

    fn is_template_with_expr(expr: &Expr) -> bool {
        matches!(crate::checkers::unwrap_paren(expr), Expr::Tpl(tpl) if !tpl.exprs.is_empty())
    }

    fn infer_pattern_from_template(
        tpl: &swc_ecma_ast::Tpl,
        namespace: &Option<String>,
    ) -> Option<String> {
        let mut pattern = String::new();
        for (i, quasi) in tpl.quasis.iter().enumerate() {
            if let Some(cooked) = &quasi.cooked
                && let Some(s) = cooked.as_str()
            {
                pattern.push_str(s);
            }
            if i < tpl.quasis.len() - 1 {
                pattern.push('*');
            }
        }

        let full_pattern = match namespace {
            Some(ns) => format!("{}.{}", ns, pattern),
            None => pattern,
        };

        let segments: Vec<&str> = full_pattern.split('.').collect();
        if segments.first().is_some_and(|s| *s == "*") && segments.len() > 1 {
            return None;
        }

        let all_wildcards = segments.iter().all(|s| *s == "*");
        if all_wildcards {
            return None;
        }

        let wildcard_count = full_pattern.matches('*').count();
        if wildcard_count > 2 {
            return None;
        }

        Some(full_pattern)
    }

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

    fn extract_object_access_name(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Member(member) if member.prop.is_computed() => match &*member.obj {
                Expr::Ident(obj_ident) => Some(obj_ident.sym.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    fn register_translation_props_from_params(
        &mut self,
        component_name: &str,
        params: &[swc_ecma_ast::Param],
    ) {
        let Some(first_param) = params.first() else {
            return;
        };
        self.register_translation_props_from_pat(component_name, &first_param.pat);
    }

    fn register_translation_props_from_pat(&mut self, component_name: &str, pat: &Pat) {
        match pat {
            Pat::Object(obj_pat) => {
                for prop in &obj_pat.props {
                    if let ObjectPatProp::KeyValue(kv) = prop
                        && let swc_ecma_ast::PropName::Ident(key_ident) = &kv.key
                    {
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
                        let prop_name = assign.key.sym.to_string();
                        self.try_register_translation_prop(component_name, &prop_name, &prop_name);
                    }
                }
            }
            Pat::Ident(_) => {}
            _ => {}
        }
    }

    fn extract_binding_name_from_pat(pat: &Pat) -> Option<String> {
        match pat {
            Pat::Ident(ident) => Some(ident.id.sym.to_string()),
            Pat::Assign(assign) => {
                if let Pat::Ident(ident) = &*assign.left {
                    Some(ident.id.sym.to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn try_register_translation_prop(
        &mut self,
        component_name: &str,
        prop_name: &str,
        binding_name: &str,
    ) {
        use super::key_objects::make_translation_prop_key;

        let key = make_translation_prop_key(component_name, prop_name);

        if let Some(translation_prop) = self.registries.translation_prop.get(&key) {
            self.binding_context.insert_binding(
                binding_name.to_string(),
                TranslationSource::FromProps {
                    namespaces: translation_prop.namespaces.clone(),
                },
            );
        }
    }

    fn register_translation_fn_params(&mut self, fn_name: &str, params: &[Pat]) {
        let is_default_export = self
            .registries
            .default_exports
            .get(self.file_path)
            .map(|s| s.as_str())
            == Some(fn_name);

        let mut registered_names = HashSet::new();

        for (idx, param) in params.iter().enumerate() {
            if let Pat::Ident(ident) = param {
                let param_name = ident.id.sym.to_string();
                let key = make_translation_fn_call_key(self.file_path, fn_name, idx);

                let default_key = if is_default_export {
                    Some(make_translation_fn_call_key(self.file_path, "default", idx))
                } else {
                    None
                };

                let fn_call = self.registries.translation_fn_call.get(&key).or_else(|| {
                    default_key
                        .as_ref()
                        .and_then(|k| self.registries.translation_fn_call.get(k))
                });

                if let Some(fn_call) = fn_call {
                    self.binding_context.insert_binding(
                        param_name.clone(),
                        TranslationSource::FromFnCall {
                            namespaces: fn_call.namespaces.clone(),
                        },
                    );
                    registered_names.insert(param_name);
                }
            }
        }

        for param in params {
            for name in extract_binding_names(param) {
                if registered_names.contains(&name) {
                    continue;
                }

                if self.binding_context.is_in_current_scope(&name) {
                    continue;
                }

                if self.binding_context.has_outer_binding(&name) {
                    self.binding_context
                        .insert_binding(name, TranslationSource::Shadowed);
                }
            }
        }
    }

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

    /// Extract the first parameter name from an arrow function.
    /// Used for iterator detection: array.map(item => ...) -> Some("item")
    fn extract_arrow_first_param(expr: &Expr) -> Option<String> {
        if let Expr::Arrow(arrow) = expr
            && let Some(first_param) = arrow.params.first()
            && let Pat::Ident(ident) = first_param
        {
            return Some(ident.id.sym.to_string());
        }
        None
    }
}

impl<'a> Visit for FileAnalyzer<'a> {
    fn visit_jsx_element(&mut self, node: &JSXElement) {
        // Visit opening element (attributes)
        node.opening.visit_with(self);

        // Check if this is a <style> tag (from HardcodedChecker)
        let is_style = if let JSXElementName::Ident(ident) = &node.opening.name {
            ident.sym == "style"
        } else {
            false
        };

        if is_style {
            if let Some(closing) = &node.closing {
                closing.visit_with(self);
            }
            return;
        }

        // Visit children with JSX context state
        let prev_state = self.jsx_state;
        self.jsx_state = prev_state.for_children();

        for child in &node.children {
            child.visit_with(self);
        }
        self.jsx_state = prev_state;

        // Visit closing element
        if let Some(closing) = &node.closing {
            closing.visit_with(self);
        }
    }

    fn visit_jsx_fragment(&mut self, node: &JSXFragment) {
        node.opening.visit_with(self);

        let prev_state = self.jsx_state;
        self.jsx_state = prev_state.for_children();

        for child in &node.children {
            child.visit_with(self);
        }
        self.jsx_state = prev_state;

        node.closing.visit_with(self);
    }

    fn visit_jsx_text(&mut self, node: &JSXText) {
        let raw_value = &node.value;
        let trimmed = raw_value.trim();

        if trimmed.is_empty() {
            return;
        }

        let trim_start_offset = raw_value.len() - raw_value.trim_start().len();
        let actual_pos = node.span.lo + swc_common::BytePos(trim_start_offset as u32);
        let loc = self.source_map.lookup_char_pos(actual_pos);

        self.check_hardcoded_line(trimmed, loc);
    }

    fn visit_jsx_expr_container(&mut self, node: &JSXExprContainer) {
        let prev_state = self.jsx_state;
        self.jsx_state.in_expr = true;

        // HardcodedChecker logic: check expressions for string literals
        // Only check if we're not in an attribute, or if we're in a checked attribute
        // Use prev_state because we need to check the state BEFORE entering the expression
        if (!prev_state.in_attr || prev_state.in_checked_attr)
            && let JSXExpr::Expr(expr) = &node.expr
        {
            self.check_expr(expr);
        }

        // Continue visiting (will call visit_call_expr for t() calls)
        node.visit_children_with(self);

        self.jsx_state = prev_state;
    }

    fn visit_jsx_attr(&mut self, node: &JSXAttr) {
        let attr_name = match &node.name {
            JSXAttrName::Ident(ident) => ident.sym.to_string(),
            JSXAttrName::JSXNamespacedName(ns) => {
                format!("{}-{}", ns.ns.sym, ns.name.sym)
            }
        };

        let prev_state = self.jsx_state;

        // Always set in_attr when visiting any attribute
        self.jsx_state.in_attr = true;

        // Only set in_checked_attr for attributes we want to check for hardcoded text
        if self.checked_attributes.contains(&attr_name) {
            self.jsx_state.in_checked_attr = true;

            // Hardcoded detection (only for string values)
            if let Some(JSXAttrValue::Str(s)) = &node.value
                && let Some(value) = s.value.as_str()
            {
                let loc = self.source_map.lookup_char_pos(s.span.lo);
                self.check_hardcoded_line(value, loc);
            }
        }

        // Always visit attribute value (for translation key extraction)
        if let Some(value) = &node.value {
            value.visit_children_with(self);
        }

        self.jsx_state = prev_state;
    }

    fn visit_fn_decl(&mut self, node: &FnDecl) {
        self.binding_context.enter_scope();
        let fn_name = node.ident.sym.to_string();

        self.register_translation_props_from_params(&fn_name, &node.function.params);

        let params: Vec<Pat> = node.function.params.iter().map(|p| p.pat.clone()).collect();
        self.register_translation_fn_params(&fn_name, &params);

        node.function.visit_children_with(self);
        self.binding_context.exit_scope();
    }

    fn visit_export_default_decl(&mut self, node: &swc_ecma_ast::ExportDefaultDecl) {
        if let DefaultDecl::Fn(fn_expr) = &node.decl {
            self.binding_context.enter_scope();

            let fn_name = fn_expr
                .ident
                .as_ref()
                .map(|i| i.sym.to_string())
                .unwrap_or_else(|| "default".to_string());

            self.register_translation_props_from_params(&fn_name, &fn_expr.function.params);

            let params: Vec<Pat> = fn_expr
                .function
                .params
                .iter()
                .map(|p| p.pat.clone())
                .collect();
            self.register_translation_fn_params(&fn_name, &params);

            fn_expr.function.visit_children_with(self);
            self.binding_context.exit_scope();
            return;
        }

        node.visit_children_with(self);
    }

    fn visit_export_default_expr(&mut self, node: &swc_ecma_ast::ExportDefaultExpr) {
        if let Expr::Arrow(arrow) = &*node.expr {
            self.binding_context.enter_scope();

            let fn_name = "default";

            if let Some(first_param) = arrow.params.first() {
                self.register_translation_props_from_pat(fn_name, first_param);
            }

            self.register_translation_fn_params(fn_name, &arrow.params);

            arrow.visit_children_with(self);
            self.binding_context.exit_scope();
            return;
        }

        node.visit_children_with(self);
    }

    fn visit_function(&mut self, node: &swc_ecma_ast::Function) {
        self.binding_context.enter_scope();
        node.visit_children_with(self);
        self.binding_context.exit_scope();
    }

    fn visit_arrow_expr(&mut self, node: &swc_ecma_ast::ArrowExpr) {
        self.binding_context.enter_scope();
        node.visit_children_with(self);
        self.binding_context.exit_scope();
    }

    fn visit_var_decl(&mut self, node: &VarDecl) {
        for decl in &node.decls {
            if let Some((fn_name, arrow)) = Self::extract_arrow_component(decl) {
                self.binding_context.enter_scope();

                if let Some(first_param) = arrow.params.first() {
                    self.register_translation_props_from_pat(&fn_name, first_param);
                }

                self.register_translation_fn_params(&fn_name, &arrow.params);

                arrow.visit_children_with(self);
                self.binding_context.exit_scope();
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
                        self.binding_context
                            .insert_binding(var_name, TranslationSource::Direct { namespace });
                    }
                }

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

            if let Some(translation_source) = self.binding_context.get_binding(fn_name).cloned()
                && !translation_source.is_shadowed()
            {
                let namespace = translation_source.primary_namespace();
                let loc = self.source_map.lookup_char_pos(node.span.lo);

                if let Some(arg) = node.args.first() {
                    let source = self.value_analyzer.analyze_expr(&arg.expr);
                    let source_line = loc
                        .file
                        .get_line(loc.line - 1)
                        .map(|cow| cow.to_string())
                        .unwrap_or_default();

                    let is_resolvable = source.resolve_keys().is_ok();

                    self.resolved_keys.push(ResolvedKey {
                        file_path: self.file_path.to_string(),
                        line: loc.line,
                        col: loc.col_display + 1,
                        source_line: source_line.clone(),
                        namespace: namespace.clone(),
                        source,
                    });

                    match crate::checkers::unwrap_paren(&arg.expr) {
                        Expr::Lit(Lit::Str(s)) => {
                            if let Some(key) = s.value.as_str() {
                                self.add_used_keys_with_namespaces(loc, key, &translation_source);
                            }
                        }
                        Expr::Tpl(tpl) if tpl.exprs.is_empty() => {
                            if let Some(quasi) = tpl.quasis.first()
                                && let Some(cooked) = &quasi.cooked
                                && let Some(key) = cooked.as_str()
                            {
                                self.add_used_keys_with_namespaces(loc, key, &translation_source);
                            }
                        }
                        Expr::Cond(cond) => {
                            self.process_ternary_arg(cond, loc, &translation_source, is_resolvable);
                        }
                        _ if !is_resolvable => {
                            let annotation_data = self
                                .annotation_store
                                .get_annotation(loc.line)
                                .map(|ann| (ann.keys.clone(), ann.relative_patterns.clone()));

                            if let Some((keys, relative_patterns)) = annotation_data {
                                for key in keys {
                                    self.add_used_key(loc.clone(), key);
                                }

                                let expanded_relative = AnnotationStore::expand_relative_patterns(
                                    &relative_patterns,
                                    &translation_source,
                                    self.available_keys,
                                );
                                for key in expanded_relative {
                                    self.add_used_key(loc.clone(), key);
                                }
                            } else {
                                let unwrapped = crate::checkers::unwrap_paren(&arg.expr);
                                let (reason, hint, pattern) = if let Expr::Tpl(tpl) = unwrapped {
                                    let pattern = if translation_source.is_indirect() {
                                        Self::infer_pattern_from_template(tpl, &None)
                                            .map(|p| format!(".{}", p))
                                    } else {
                                        Self::infer_pattern_from_template(tpl, &namespace)
                                    };
                                    let hint = pattern.as_ref().map(|p| {
                                        if self.should_use_jsx_comment(&source_line) {
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
                                    (DynamicKeyReason::TemplateWithExpr, hint, pattern)
                                } else {
                                    (DynamicKeyReason::VariableKey, None, None)
                                };
                                self.add_warning(loc, reason, hint, pattern);
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
                        self.binding_context
                            .get_binding(t_var)
                            .is_some_and(|src| !src.is_shadowed())
                    }
                    _ => true,
                };

                if should_record {
                    let namespace = match &*arg.expr {
                        Expr::Ident(t_ident) => {
                            let t_var = t_ident.sym.as_str();
                            self.binding_context
                                .get_binding(t_var)
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

        // Handle method calls: t.raw("key"), t.rich("key"), t.markup("key")
        if let Callee::Expr(expr) = &node.callee
            && let Expr::Member(member) = &**expr
            && let Expr::Ident(obj_ident) = &*member.obj
            && let MemberProp::Ident(method_ident) = &member.prop
        {
            let obj_name = obj_ident.sym.as_str();
            let method_name = method_ident.sym.as_str();

            if matches!(method_name, "raw" | "rich" | "markup")
                && let Some(translation_source) =
                    self.binding_context.get_binding(obj_name).cloned()
                && !translation_source.is_shadowed()
                && let Some(arg) = node.args.first()
            {
                let loc = self.source_map.lookup_char_pos(node.span.lo);

                match crate::checkers::unwrap_paren(&arg.expr) {
                    Expr::Lit(Lit::Str(s)) => {
                        if let Some(key) = s.value.as_str() {
                            self.add_used_keys_with_namespaces(
                                loc.clone(),
                                key,
                                &translation_source,
                            );
                        }
                    }
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
                    _ => {}
                }
            }
        }

        // Detect iterator patterns (array.map, forEach, etc.)
        // This enables resolving keys in patterns like: KEYS.map(k => t(`prefix.${k}`))
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

        if entered_scope {
            self.value_analyzer.exit_scope();
        }
    }
}

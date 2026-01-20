//! Core AST visitor for translation key extraction.
//!
//! Contains the `TranslationKeyVisitor` struct and its `Visit` trait implementation
//! for detecting translation keys used in code.

use std::collections::HashSet;

use swc_common::{Loc, SourceMap, comments::SingleThreadedComments};
use swc_ecma_ast::{
    CallExpr, Callee, CondExpr, Expr, FnDecl, JSXElement, JSXExprContainer, JSXFragment, Lit,
    MemberProp, Module, ObjectPatProp, Pat, VarDecl, VarDeclarator,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::annotation_store::AnnotationStore;
use super::binding_context::BindingContext;
use super::result::{DynamicKeyReason, DynamicKeyWarning, KeyExtractionResult, UsedKey};
use super::translation_source::TranslationSource;
use crate::checkers::{
    extract_namespace_from_call, is_translation_hook,
    key_objects::{
        FileImports, extract_binding_names, make_translation_fn_call_key, make_translation_prop_key,
    },
    schema::SchemaCallInfo,
    value_analyzer::ValueAnalyzer,
    value_source::ResolvedKey,
};
use crate::commands::context::Registries;
use crate::directives::{DisableContext, DisableRule};

pub(crate) fn resolve_full_key(namespace: &Option<String>, key: &str) -> String {
    match namespace {
        Some(ns) => format!("{}.{}", ns, key),
        _ => key.to_string(),
    }
}

/// Extracts translation keys from TSX/JSX files.
///
/// Analyzes source code to find translation keys that are used,
/// tracking namespace bindings and detecting dynamic keys.
pub struct TranslationKeyVisitor<'a> {
    file_path: &'a str,
    source_map: &'a SourceMap,

    // Composed modules
    binding_context: BindingContext,
    annotation_store: AnnotationStore,
    value_analyzer: ValueAnalyzer<'a>,
    disable_context: DisableContext,

    // Dependencies
    registries: &'a Registries,
    available_keys: &'a HashSet<String>,

    // Results
    pub used_keys: Vec<UsedKey>,
    pub warnings: Vec<DynamicKeyWarning>,
    pub schema_calls: Vec<SchemaCallInfo>,
    pub resolved_keys: Vec<ResolvedKey>,

    // State
    in_jsx_context: bool,
    in_jsx_expr: bool,
}

impl<'a> TranslationKeyVisitor<'a> {
    pub fn new(
        file_path: &'a str,
        source_map: &'a SourceMap,
        comments: &SingleThreadedComments,
        registries: &'a Registries,
        file_imports: &'a FileImports,
        source: &str,
        available_keys: &'a HashSet<String>,
    ) -> Self {
        // Parse glot-message-keys annotations
        let annotation_store = AnnotationStore::parse(source, file_path, available_keys);
        // Parse glot-disable directives
        let disable_context = DisableContext::from_comments(comments, source_map);

        Self {
            file_path,
            source_map,
            binding_context: BindingContext::new(),
            annotation_store,
            value_analyzer: ValueAnalyzer::new(
                file_path,
                &registries.key_object,
                &registries.key_array,
                &registries.string_array,
                file_imports,
            ),
            disable_context,
            registries,
            available_keys,
            used_keys: Vec::new(),
            warnings: Vec::new(),
            schema_calls: Vec::new(),
            resolved_keys: Vec::new(),
            in_jsx_context: false,
            in_jsx_expr: false,
        }
    }

    fn should_use_jsx_comment(&self, source_line: &str) -> bool {
        if !self.in_jsx_context {
            return false;
        }
        if self.in_jsx_expr {
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
        let in_jsx_context = self.should_use_jsx_comment(&source_line);
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

    /// Add used keys for a relative key with all namespaces from the translation source.
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
        let in_jsx_context = self.should_use_jsx_comment(&source_line);
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

    /// Process a ternary expression: t(cond ? "keyA" : "keyB")
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

    /// Infer a glob pattern from a template literal.
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

    fn extract_arrow_first_param(expr: &Expr) -> Option<String> {
        if let Expr::Arrow(arrow) = expr
            && let Some(first_param) = arrow.params.first()
            && let Pat::Ident(ident) = first_param
        {
            return Some(ident.id.sym.to_string());
        }
        None
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

        // First pass: register FromFnCall bindings
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

        // Second pass: shadow parameters that have outer bindings
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

    /// Main entry point: extract translation keys from a module.
    pub fn extract(mut self, module: &Module) -> KeyExtractionResult {
        self.visit_module(module);
        KeyExtractionResult {
            used_keys: self.used_keys,
            warnings: self.warnings,
            schema_calls: self.schema_calls,
            resolved_keys: self.resolved_keys,
            pattern_warnings: self.annotation_store.warnings,
        }
    }
}

impl<'a> Visit for TranslationKeyVisitor<'a> {
    fn visit_jsx_expr_container(&mut self, node: &JSXExprContainer) {
        let prev = self.in_jsx_expr;
        self.in_jsx_expr = true;
        node.visit_children_with(self);
        self.in_jsx_expr = prev;
    }

    fn visit_jsx_element(&mut self, node: &JSXElement) {
        node.opening.visit_with(self);

        let prev = self.in_jsx_context;
        self.in_jsx_context = true;
        for child in &node.children {
            child.visit_with(self);
        }
        self.in_jsx_context = prev;

        if let Some(closing) = &node.closing {
            closing.visit_with(self);
        }
    }

    fn visit_jsx_fragment(&mut self, node: &JSXFragment) {
        node.opening.visit_with(self);

        let prev = self.in_jsx_context;
        self.in_jsx_context = true;
        for child in &node.children {
            child.visit_with(self);
        }
        self.in_jsx_context = prev;

        node.closing.visit_with(self);
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
        if let swc_ecma_ast::DefaultDecl::Fn(fn_expr) = &node.decl {
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

        // Detect iterator patterns
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

//! Combined collector for schema functions and key objects.
//!
//! This module merges SchemaFunctionCollector and KeyObjectCollector to reduce
//! AST traversal overhead by visiting each file only once during registry building.

use std::collections::HashMap;

use swc_ecma_ast::{
    ArrowExpr, CallExpr, Callee, Decl, DefaultDecl, Expr, Function, ImportSpecifier, JSXAttr,
    JSXAttrName, JSXAttrOrSpread, JSXAttrValue, JSXElement, JSXElementName, JSXExpr, Lit,
    ModuleExportName, ObjectLit, Pat, TsEntityName, TsType, VarDecl, VarDeclarator,
};
use swc_ecma_visit::{Visit, VisitWith};

use super::{
    extract_namespace_from_call, is_translation_hook,
    key_objects::{
        FileImports, ImportInfo, KeyArray, KeyObject, StringArray, TranslationBindingValue,
        TranslationFnCall, TranslationProp, extract_binding_names,
    },
    schema::{SchemaFunction, SchemaFunctionContext},
};

/// Combined collector that gathers both schema functions and key objects in a single AST traversal.
pub struct RegistryCollector {
    // Shared fields
    pub file_path: String,
    /// Tracks nesting depth: 0 = module level, >0 = inside function/arrow
    scope_depth: usize,

    // From SchemaFunctionCollector
    pub schema_functions: Vec<SchemaFunction>,
    current_schema_function: Option<SchemaFunctionContext>,
    /// Depth of nested functions that shadow the schema t_param_name.
    schema_shadow_depth: usize,

    // From KeyObjectCollector
    pub objects: Vec<KeyObject>,
    pub arrays: Vec<KeyArray>,
    pub string_arrays: Vec<StringArray>,
    pub imports: FileImports,
    pub translation_props: Vec<TranslationProp>,
    pub translation_fn_calls: Vec<TranslationFnCall>,
    pub default_export_name: Option<String>,
    /// Stack of translation function bindings scoped by function/arrow.
    translation_bindings_stack: Vec<HashMap<String, TranslationBindingValue>>,
}

impl RegistryCollector {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            scope_depth: 0,
            schema_functions: Vec::new(),
            current_schema_function: None,
            schema_shadow_depth: 0,
            objects: Vec::new(),
            arrays: Vec::new(),
            string_arrays: Vec::new(),
            imports: Vec::new(),
            translation_props: Vec::new(),
            translation_fn_calls: Vec::new(),
            default_export_name: None,
            translation_bindings_stack: vec![HashMap::new()],
        }
    }

    // ============================================================
    // Schema-related methods (from SchemaFunctionCollector)
    // ============================================================

    fn check_schema_function(&mut self, decl: &VarDeclarator) {
        let func_name = match &decl.name {
            Pat::Ident(ident) => ident.id.sym.to_string(),
            _ => return,
        };

        let arrow = match &decl.init {
            Some(expr) => match &**expr {
                Expr::Arrow(arrow) => arrow,
                _ => return,
            },
            _ => return,
        };

        let t_param = self.extract_t_param(arrow);

        let t_param_name = match t_param {
            Some(name) => name,
            None => return,
        };

        self.current_schema_function = Some(SchemaFunctionContext {
            name: func_name.clone(),
            t_param_name: t_param_name.clone(),
            keys: Vec::new(),
            nested_calls: Vec::new(),
        });

        arrow.body.visit_with(self);

        if let Some(ctx) = self.current_schema_function.take()
            && (!ctx.keys.is_empty() || !ctx.nested_calls.is_empty())
        {
            self.schema_functions.push(SchemaFunction {
                name: ctx.name,
                file_path: self.file_path.clone(),
                keys: ctx.keys,
                nested_calls: ctx.nested_calls,
            });
        }
    }

    fn extract_t_param(&self, arrow: &ArrowExpr) -> Option<String> {
        arrow.params.first().and_then(|param| {
            if let Pat::Ident(ident) = param {
                let name = ident.id.sym.to_string();
                let has_tfunction_type = ident
                    .type_ann
                    .as_ref()
                    .map(|ann| is_tfunction_type(&ann.type_ann))
                    .unwrap_or(false);
                if name.starts_with('t') || has_tfunction_type {
                    Some(name)
                } else {
                    None
                }
            } else {
                None
            }
        })
    }

    fn check_schema_call_expr(&mut self, node: &CallExpr) {
        // Only collect keys if not inside a shadowed scope
        if self.schema_shadow_depth == 0
            && let Some(ctx) = &mut self.current_schema_function
            && let Callee::Expr(expr) = &node.callee
            && let Expr::Ident(ident) = &**expr
        {
            let fn_name = ident.sym.as_str();
            if fn_name == ctx.t_param_name {
                if let Some(key) = extract_string_arg(node) {
                    ctx.keys.push(key);
                }
            } else if is_schema_call_with_t(node, &ctx.t_param_name) {
                ctx.nested_calls.push(fn_name.to_string());
            }
        }
    }

    // ============================================================
    // Key object related methods (from KeyObjectCollector)
    // ============================================================

    fn enter_binding_scope(&mut self) {
        self.translation_bindings_stack.push(HashMap::new());
    }

    fn exit_binding_scope(&mut self) {
        self.translation_bindings_stack.pop();
    }

    fn insert_translation_binding(&mut self, name: String, namespace: Option<String>) {
        if let Some(scope) = self.translation_bindings_stack.last_mut() {
            scope.insert(name, TranslationBindingValue::Translation(namespace));
        }
    }

    fn shadow_binding(&mut self, name: String) {
        if let Some(scope) = self.translation_bindings_stack.last_mut() {
            scope.insert(name, TranslationBindingValue::Shadowed);
        }
    }

    fn get_translation_binding(&self, name: &str) -> Option<Option<String>> {
        for scope in self.translation_bindings_stack.iter().rev() {
            if let Some(value) = scope.get(name) {
                return match value {
                    TranslationBindingValue::Translation(namespace) => Some(namespace.clone()),
                    TranslationBindingValue::Shadowed => None,
                };
            }
        }
        None
    }

    fn check_var_decl(&mut self, node: &VarDecl, is_exported: bool, is_module_level: bool) {
        for decl in &node.decls {
            let name = match &decl.name {
                Pat::Ident(ident) => ident.id.sym.to_string(),
                _ => continue,
            };

            let Some(init) = &decl.init else { continue };

            // Track translation function bindings
            self.check_translation_binding(&name, init);

            let inner_expr = unwrap_ts_expr(init);

            match inner_expr {
                Expr::Object(obj) => {
                    if let Some(candidate_keys) = extract_string_value(obj) {
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
                    // Try as string array first
                    if let Some(values) = extract_string_array(arr) {
                        self.string_arrays.push(StringArray {
                            name,
                            file_path: self.file_path.clone(),
                            is_exported,
                            is_module_level,
                            values,
                        });
                    }
                    // Otherwise try as object array
                    else if let Some(property_values) = extract_array_properties(arr) {
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

    fn add_or_update_translation_prop(
        &mut self,
        component_name: &str,
        prop_name: &str,
        namespace: Option<String>,
    ) {
        if let Some(existing) = self
            .translation_props
            .iter_mut()
            .find(|p| p.component_name == component_name && p.prop_name == prop_name)
        {
            if !existing.namespaces.contains(&namespace) {
                existing.namespaces.push(namespace);
            }
        } else {
            self.translation_props.push(TranslationProp {
                component_name: component_name.to_string(),
                prop_name: prop_name.to_string(),
                namespaces: vec![namespace],
            });
        }
    }

    fn add_or_update_translation_fn_call(
        &mut self,
        fn_file_path: &str,
        fn_name: &str,
        arg_index: usize,
        namespace: Option<String>,
    ) {
        if let Some(existing) = self.translation_fn_calls.iter_mut().find(|c| {
            c.fn_file_path == fn_file_path && c.fn_name == fn_name && c.arg_index == arg_index
        }) {
            if !existing.namespaces.contains(&namespace) {
                existing.namespaces.push(namespace);
            }
        } else {
            self.translation_fn_calls.push(TranslationFnCall {
                fn_file_path: fn_file_path.to_string(),
                fn_name: fn_name.to_string(),
                arg_index,
                namespaces: vec![namespace],
            });
        }
    }

    fn resolve_fn_definition(&self, local_fn_name: &str) -> (String, String) {
        use super::key_objects::resolve_import_path;
        use std::path::Path;

        if let Some(import) = self.imports.iter().find(|i| i.local_name == local_fn_name) {
            let file_path = resolve_import_path(Path::new(&self.file_path), &import.module_path)
                .unwrap_or_else(|| self.file_path.clone());
            (file_path, import.imported_name.clone())
        } else {
            (self.file_path.clone(), local_fn_name.to_string())
        }
    }

    fn extract_jsx_member_name(member: &swc_ecma_ast::JSXMemberExpr) -> String {
        let object_name = match &member.obj {
            swc_ecma_ast::JSXObject::Ident(ident) => ident.sym.to_string(),
            swc_ecma_ast::JSXObject::JSXMemberExpr(nested) => Self::extract_jsx_member_name(nested),
        };
        format!("{}.{}", object_name, member.prop.sym)
    }
}

impl Visit for RegistryCollector {
    fn visit_export_decl(&mut self, node: &swc_ecma_ast::ExportDecl) {
        if let Decl::Var(var_decl) = &node.decl {
            // Schema logic: check for schema functions
            for decl in &var_decl.decls {
                self.check_schema_function(decl);
            }

            // Key object logic: collect exported variables
            let is_module_level = self.scope_depth == 0;
            self.check_var_decl(var_decl, true, is_module_level);
        } else {
            node.visit_children_with(self);
        }
    }

    fn visit_export_default_decl(&mut self, node: &swc_ecma_ast::ExportDefaultDecl) {
        match &node.decl {
            DefaultDecl::Fn(fn_expr) => {
                self.default_export_name = fn_expr
                    .ident
                    .as_ref()
                    .map(|ident| ident.sym.to_string())
                    .or_else(|| Some("default".to_string()));
            }
            DefaultDecl::Class(class_expr) => {
                self.default_export_name = class_expr
                    .ident
                    .as_ref()
                    .map(|ident| ident.sym.to_string())
                    .or_else(|| Some("default".to_string()));
            }
            _ => {}
        }
        node.visit_children_with(self);
    }

    fn visit_export_default_expr(&mut self, node: &swc_ecma_ast::ExportDefaultExpr) {
        match &*node.expr {
            Expr::Ident(ident) => {
                self.default_export_name = Some(ident.sym.to_string());
            }
            Expr::Arrow(_) | Expr::Fn(_) => {
                self.default_export_name = Some("default".to_string());
            }
            _ => {}
        }
        node.visit_children_with(self);
    }

    fn visit_var_decl(&mut self, node: &VarDecl) {
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

    fn visit_function(&mut self, node: &Function) {
        // KeyObjectCollector logic
        self.scope_depth += 1;
        self.enter_binding_scope();

        for param in &node.params {
            for name in extract_binding_names(&param.pat) {
                self.shadow_binding(name);
            }
        }

        // SchemaFunctionCollector logic
        let shadows_t = self.current_schema_function.as_ref().is_some_and(|ctx| {
            node.params.iter().any(|param| {
                if let Pat::Ident(ident) = &param.pat {
                    ident.id.sym.as_str() == ctx.t_param_name
                } else {
                    false
                }
            })
        });

        if shadows_t {
            self.schema_shadow_depth += 1;
        }

        node.visit_children_with(self);

        if shadows_t {
            self.schema_shadow_depth -= 1;
        }

        self.exit_binding_scope();
        self.scope_depth -= 1;
    }

    fn visit_arrow_expr(&mut self, node: &ArrowExpr) {
        // KeyObjectCollector logic
        self.scope_depth += 1;
        self.enter_binding_scope();

        for param in &node.params {
            for name in extract_binding_names(param) {
                self.shadow_binding(name);
            }
        }

        // SchemaFunctionCollector logic
        let shadows_t = self.current_schema_function.as_ref().is_some_and(|ctx| {
            node.params.iter().any(|param| {
                if let Pat::Ident(ident) = param {
                    ident.id.sym.as_str() == ctx.t_param_name
                } else {
                    false
                }
            })
        });

        if shadows_t {
            self.schema_shadow_depth += 1;
        }

        node.visit_children_with(self);

        if shadows_t {
            self.schema_shadow_depth -= 1;
        }

        self.exit_binding_scope();
        self.scope_depth -= 1;
    }

    fn visit_call_expr(&mut self, node: &CallExpr) {
        // Schema logic
        self.check_schema_call_expr(node);

        // KeyObjectCollector logic: detect translation functions passed as arguments
        if let Callee::Expr(callee_expr) = &node.callee
            && let Expr::Ident(fn_ident) = &**callee_expr
        {
            let local_fn_name = fn_ident.sym.to_string();

            if !is_translation_hook(&local_fn_name) {
                let (fn_file_path, fn_name) = self.resolve_fn_definition(&local_fn_name);

                for (idx, arg) in node.args.iter().enumerate() {
                    if let Expr::Ident(arg_ident) = &*arg.expr {
                        let var_name = arg_ident.sym.to_string();

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
        }

        node.visit_children_with(self);
    }

    fn visit_jsx_element(&mut self, node: &JSXElement) {
        let component_name = match &node.opening.name {
            JSXElementName::Ident(ident) => {
                let name = ident.sym.to_string();
                if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    Some(name)
                } else {
                    None
                }
            }
            JSXElementName::JSXMemberExpr(member) => Some(Self::extract_jsx_member_name(member)),
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

        node.visit_children_with(self);
    }
}

// ============================================================
// Helper functions
// ============================================================

fn is_tfunction_type(ts_type: &TsType) -> bool {
    match ts_type {
        TsType::TsTypeRef(type_ref) => {
            if let TsEntityName::Ident(ident) = &type_ref.type_name {
                ident.sym.as_str() == "TFunction"
            } else {
                false
            }
        }
        _ => false,
    }
}

fn extract_string_arg(call: &CallExpr) -> Option<String> {
    call.args.first().and_then(|arg| match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => s.value.as_str().map(|s| s.to_string()),
        Expr::Tpl(tpl) if tpl.exprs.is_empty() => tpl
            .quasis
            .first()
            .and_then(|q| q.cooked.as_ref())
            .and_then(|s| s.as_str().map(|s| s.to_string())),
        _ => None,
    })
}

fn is_schema_call_with_t(call: &CallExpr, t_param_name: &str) -> bool {
    call.args.first().is_some_and(|arg| {
        if let Expr::Ident(ident) = &*arg.expr {
            ident.sym.as_str() == t_param_name
        } else {
            false
        }
    })
}

fn unwrap_ts_expr(expr: &Expr) -> &Expr {
    match expr {
        Expr::TsAs(ts_as) => unwrap_ts_expr(&ts_as.expr),
        Expr::TsConstAssertion(ts_const) => unwrap_ts_expr(&ts_const.expr),
        Expr::TsSatisfies(ts_sat) => unwrap_ts_expr(&ts_sat.expr),
        Expr::Paren(paren) => unwrap_ts_expr(&paren.expr),
        _ => expr,
    }
}

fn extract_string_value(obj: &ObjectLit) -> Option<Vec<String>> {
    use swc_ecma_ast::{Prop, PropName, PropOrSpread};

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

fn extract_string_array(arr: &swc_ecma_ast::ArrayLit) -> Option<Vec<String>> {
    use swc_ecma_ast::ExprOrSpread;

    let mut values = Vec::new();

    for elem in &arr.elems {
        match elem {
            Some(ExprOrSpread {
                spread: Some(_), ..
            }) => {
                return None;
            }
            Some(ExprOrSpread { expr, spread: None }) => {
                let inner = unwrap_ts_expr(expr);
                match inner {
                    Expr::Lit(Lit::Str(s)) => {
                        if let Some(v) = s.value.as_str() {
                            values.push(v.to_string());
                        } else {
                            return None;
                        }
                    }
                    _ => {
                        return None;
                    }
                }
            }
            None => {}
        }
    }

    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn extract_array_properties(arr: &swc_ecma_ast::ArrayLit) -> Option<HashMap<String, Vec<String>>> {
    use swc_ecma_ast::{ExprOrSpread, Prop, PropOrSpread};

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
                            && let Some(prop_name) = extract_prop_name(&kv.key)
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

fn extract_prop_name(key: &swc_ecma_ast::PropName) -> Option<String> {
    use swc_ecma_ast::PropName;

    match key {
        PropName::Ident(ident) => Some(ident.sym.to_string()),
        PropName::Str(s) => s.value.as_str().map(|s| s.to_string()),
        _ => None,
    }
}

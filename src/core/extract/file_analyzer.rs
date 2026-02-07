//! Combined file analyzer that extracts translation keys and detects hardcoded text in a single AST traversal.
//!
//! This module merges the functionality of:
//! - `HardcodedChecker`: detects hardcoded text in JSX/TSX
//! - `TranslationCallCollector`: collects raw translation calls for later resolution
//! - `SchemaCallCollector`: collects schema function calls
//!
//! By combining these into a single visitor, we reduce the number of AST traversals per file
//! from 3-4 to just 1, significantly improving performance.
//!
//! The actual resolution of translation calls to ResolvedKeyUsage/UnresolvedKeyUsage
//! happens in Phase 3 (see `crate::core::resolve`).

use std::collections::HashSet;

use swc_common::{Loc, SourceMap, Spanned};
use swc_ecma_ast::{
    BinaryOp, BlockStmtOrExpr, CallExpr, Callee, DefaultDecl, Expr, FnDecl, JSXAttr, JSXAttrName,
    JSXAttrValue, JSXElement, JSXElementName, JSXExpr, JSXExprContainer, JSXFragment, JSXText, Lit,
    MemberProp, Module, ObjectPatProp, Pat, ReturnStmt, VarDecl, VarDeclarator,
};
use swc_ecma_visit::{Visit, VisitWith};

use crate::core::collect::SuppressibleRule;
use crate::core::{CommentStyle, SourceContext, SourceLocation};
use crate::issues::HardcodedTextIssue;
use crate::utils::contains_alphabetic;

use crate::core::collect::Registries;
use crate::core::extract::{
    BindingContext, RawTranslationCall, TranslationCallKind, TranslationSource, ValueAnalyzer,
    ValueSource,
};
use crate::core::{
    collect::types::{
        FileComments, FileImports, extract_binding_names, make_translation_fn_call_key,
    },
    schema::SchemaCallInfo,
    utils::{extract_namespace_from_call, is_translation_hook},
};

/// Tracks JSX context state during AST traversal.
///
/// These flags are independent (not mutually exclusive) and help determine:
/// 1. Which hardcoded text checks to apply
/// 2. What comment style to use (JSX vs JS) for issue reporting
///
/// Used by Phase 2 (Extraction) to maintain context while walking the AST.
#[derive(Debug, Clone, Copy, Default)]
struct JsxState {
    /// Inside JSX element/fragment children (between opening and closing tags).
    in_context: bool,
    /// Inside a JSX attribute (e.g., `<div title={...}>`).
    in_attr: bool,
    /// Inside a checked attribute (placeholder, title, alt, aria-*, etc.).
    /// Only these attributes are checked for hardcoded text.
    in_checked_attr: bool,
    /// Inside a JSX expression container `{...}`.
    in_expr: bool,
    /// The line where the current expression container opens (1-indexed).
    /// Only meaningful when `in_expr` is true.
    expr_start_line: usize,
}

/// Kind of JavaScript statement that produces JSX.
///
/// Used to determine correct comment style when hardcoded text appears in
/// statement-level JSX (e.g., `return <div>text</div>` vs just `<div>text</div>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StmtKind {
    /// Return statement: `return <div>text</div>`
    Return,
    /// Variable initializer: `const x = <div>text</div>`
    VarInit,
    /// Arrow function body: `() => <div>text</div>`
    ArrowExpr,
}

/// Tracks the statement context for a specific source line.
///
/// Used to determine if JSX on a given line is part of a JS statement,
/// which affects whether we use JS or JSX comment style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StmtContext {
    /// Source line number (1-indexed).
    line: usize,
    /// The kind of statement producing JSX on this line.
    kind: StmtKind,
}

impl JsxState {
    /// Create a new state for entering JSX children.
    fn for_children(self) -> Self {
        Self {
            in_context: true,
            in_attr: false,
            in_checked_attr: false,
            in_expr: false,
            expr_start_line: 0,
        }
    }
}

/// Result of analyzing a single file (Phase 2: Extraction).
///
/// Contains raw collected data from a single AST traversal. Resolution to
/// `ResolvedKeyUsage`/`UnresolvedKeyUsage` happens in Phase 3 (see `crate::core::resolve`).
#[derive(Debug)]
pub struct FileAnalysisResult {
    /// Hardcoded text issues found in JSX/TSX (ready to report).
    pub hardcoded_issues: Vec<HardcodedTextIssue>,
    /// Raw translation calls with unresolved keys (e.g., `t("key")`, `t.raw("key")`).
    /// Phase 3 will resolve these against locale files.
    pub raw_calls: Vec<RawTranslationCall>,
    /// Schema function calls (e.g., `loginSchema(t)`) for schema validation rules.
    pub schema_calls: Vec<SchemaCallInfo>,
}

/// Combined analyzer that collects translation calls and detects hardcoded text in a single AST pass.
///
/// This struct merges the functionality of three formerly separate visitors:
/// 1. **HardcodedChecker**: Detects hardcoded text in JSX/JSX attributes
/// 2. **TranslationCallCollector**: Collects translation function calls (`t()`, `t.raw()`, etc.)
/// 3. **SchemaCallCollector**: Collects schema function calls for validation
///
/// By combining these into one visitor implementing `swc_ecma_visit::Visit`, we traverse
/// each file's AST only once instead of 3-4 times, significantly improving performance.
///
/// # Phase Context
///
/// This analyzer operates in **Phase 2: Extraction**. It takes parsed AST and registries
/// from Phase 1 (Collection) and produces:
/// - Hardcoded text issues (ready to report)
/// - Raw translation calls (unresolved keys to be resolved in Phase 3)
/// - Schema calls (for validation rules)
///
/// # Architecture
///
/// The analyzer maintains state while walking the AST:
/// - `jsx_state`: Tracks whether we're in JSX context, attributes, or expressions
/// - `binding_context`: Tracks variable bindings for translation functions (scoped)
/// - `value_analyzer`: Resolves translation key arguments (handles template literals, object access, etc.)
///
/// # Usage
///
/// ```ignore
/// use glot::core::extract::FileAnalyzer;
/// # use swc_common::SourceMap;
/// # use std::collections::HashSet;
/// # use glot::core::collect::{Registries, types::FileComments, types::FileImports};
///
/// // Assuming you have these from Phase 1
/// let source_map: &SourceMap = /* ... */;
/// let registries: &Registries = /* ... */;
/// let file_comments: &FileComments = /* ... */;
/// let file_imports: &FileImports = /* ... */;
/// let checked_attributes: &[String] = /* ... */;
/// let ignore_texts: &HashSet<String> = /* ... */;
/// let module: &Module = /* parsed AST */;
///
/// let analyzer = FileAnalyzer::new(
///     "src/app/page.tsx",
///     source_map,
///     file_comments,
///     checked_attributes,
///     ignore_texts,
///     registries,
///     file_imports,
/// );
///
/// let result = analyzer.analyze(module);
/// // result.hardcoded_issues: Vec<HardcodedTextIssue>
/// // result.raw_calls: Vec<RawTranslationCall> (to be resolved in Phase 3)
/// // result.schema_calls: Vec<SchemaCallInfo>
/// ```
pub struct FileAnalyzer<'a> {
    // ============================================================
    // Shared fields (used by all analysis types)
    // ============================================================
    /// Path to the file being analyzed (relative to source root).
    file_path: &'a str,

    /// SWC source map for looking up line/column positions.
    source_map: &'a SourceMap,

    /// Parsed comments and suppressions from Phase 1.
    file_comments: &'a FileComments,

    /// Current JSX traversal state (in context, attribute, expression, etc.).
    jsx_state: JsxState,

    /// Stack of statement contexts for determining comment style.
    stmt_context: Vec<StmtContext>,

    // ============================================================
    // Hardcoded text detection fields
    // ============================================================
    /// JSX attributes to check for hardcoded text (from config).
    /// e.g., ["placeholder", "title", "alt", "aria-label"]
    checked_attributes: &'a [String],

    /// Hardcoded text values to ignore (from config `ignoreTexts`).
    ignore_texts: &'a HashSet<String>,

    // ============================================================
    // Translation call collection fields
    // ============================================================
    /// Tracks bindings for translation functions (e.g., `t`, `tMessages`).
    /// Handles scoping, shadowing, and translation sources (direct, props, fn params).
    binding_context: BindingContext,

    /// Analyzes translation call arguments to resolve keys.
    /// Handles literals, template strings, object access, array iteration, etc.
    value_analyzer: ValueAnalyzer<'a>,

    /// Registries from Phase 1 (translation props, fn calls, key objects, etc.).
    registries: &'a Registries,

    // ============================================================
    // Output (accumulated during traversal)
    // ============================================================
    /// Hardcoded text issues collected during traversal.
    hardcoded_issues: Vec<HardcodedTextIssue>,

    /// Raw translation calls collected during traversal.
    raw_calls: Vec<RawTranslationCall>,

    /// Schema function calls collected during traversal.
    schema_calls: Vec<SchemaCallInfo>,
}

impl<'a> FileAnalyzer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file_path: &'a str,
        source_map: &'a SourceMap,
        file_comments: &'a FileComments,
        checked_attributes: &'a [String],
        ignore_texts: &'a HashSet<String>,
        registries: &'a Registries,
        file_imports: &'a FileImports,
    ) -> Self {
        Self {
            file_path,
            source_map,
            file_comments,
            jsx_state: JsxState::default(),
            stmt_context: Vec::new(),
            checked_attributes,
            ignore_texts,
            binding_context: BindingContext::new(),
            value_analyzer: ValueAnalyzer::new(
                file_path,
                &registries.key_object,
                &registries.key_array,
                &registries.string_array,
                file_imports,
            ),
            registries,
            hardcoded_issues: Vec::new(),
            raw_calls: Vec::new(),
            schema_calls: Vec::new(),
        }
    }

    /// Main entry point: analyze a module and return results.
    pub fn analyze(mut self, module: &Module) -> FileAnalysisResult {
        self.visit_module(module);
        FileAnalysisResult {
            hardcoded_issues: self.hardcoded_issues,
            raw_calls: self.raw_calls,
            schema_calls: self.schema_calls,
        }
    }

    // ============================================================
    // Hardcoded text detection methods
    // ============================================================

    /// Check if hardcoded text should be reported for a given line.
    ///
    /// Returns false if:
    /// - Line is suppressed with `glot-disable-next-line hardcoded`
    /// - Text is in the ignore list (config `ignoreTexts`)
    /// - Text contains no alphabetic characters (pure numbers/symbols)
    fn should_report_hardcoded(&self, line: usize, text: &str) -> bool {
        if self
            .file_comments
            .suppressions
            .is_suppressed(line, SuppressibleRule::Hardcoded)
        {
            return false;
        }
        let text = text.trim();
        if self.ignore_texts.contains(text) {
            return false;
        }
        contains_alphabetic(text)
    }

    /// Determine the correct comment style (JSX vs JS) for a source line.
    ///
    /// This affects the suppress comment format shown to users:
    /// - `CommentStyle::Jsx` → `{/* glot-disable-next-line */}`
    /// - `CommentStyle::Js` → `// glot-disable-next-line`
    ///
    /// # Logic
    ///
    /// 1. JSX attributes always use JS style (e.g., `<div title="text">`)
    /// 2. Ternary branches in JSX expressions use JS style (`: "text"`)
    /// 3. Statement-level JSX uses JS style (e.g., `return <div>text</div>`)
    /// 4. JSX children use JSX style (e.g., `<div>text</div>`)
    /// 5. Default to JS style for non-JSX contexts
    fn decide_comment_style(&self, source_line: &str, line: usize) -> CommentStyle {
        let trimmed_line = source_line.trim_start();

        // A) JSX attributes -> JS comment
        if self.jsx_state.in_attr {
            return CommentStyle::Js;
        }

        // B) Continuation lines inside JSX expression containers -> JS comment
        // When in_expr is true and the issue is on a different line from the
        // opening `{`, we're on a continuation line inside a multi-line expression:
        //   {currentChapterTitle ||
        //     t("chapterNumber", { number: currentChapter })}
        // Here `t(...)` needs `// ...` comment, not `{/* ... */}`.
        // When the issue is on the same line as `{` (e.g., `{t("greeting")}`),
        // the comment goes above in JSX children context → JSX style.
        if self.jsx_state.in_expr && line != self.jsx_state.expr_start_line {
            return CommentStyle::Js;
        }

        // E) JS statement-level JSX -> JS comment
        if let Some(kind) = self.statement_context_for_line(line)
            && self.should_force_js_for_statement(kind, trimmed_line)
        {
            return CommentStyle::Js;
        }

        // D) JSX children -> JSX comment
        if self.jsx_state.in_context {
            return CommentStyle::Jsx;
        }

        CommentStyle::Js
    }

    /// Record a hardcoded text issue with source context.
    fn add_hardcoded_issue(&mut self, value: &str, loc: Loc) {
        let source_line = loc
            .file
            .get_line(loc.line - 1)
            .map(|cow| cow.to_string())
            .unwrap_or_default();

        let comment_style = self.decide_comment_style(&source_line, loc.line);
        self.hardcoded_issues.push(HardcodedTextIssue {
            context: SourceContext::new(
                SourceLocation::new(self.file_path, loc.line, loc.col_display + 1),
                source_line,
                comment_style,
            ),
            text: value.to_owned(),
        });
    }

    /// Check a text value and record as hardcoded issue if it should be reported.
    fn check_hardcoded_line(&mut self, value: &str, loc: Loc) {
        if self.should_report_hardcoded(loc.line, value) {
            self.add_hardcoded_issue(value, loc);
        }
    }

    /// Check an expression for hardcoded text (string literals, templates, conditionals).
    ///
    /// Recursively walks through:
    /// - String literals: `"text"`
    /// - Template literals: `` `text ${expr}` ``
    /// - Logical operators: `condition && "text"`, `a || "text"`
    /// - Ternary operators: `condition ? "text" : "other"`
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
    // Translation call collection methods
    // ============================================================

    /// Create SourceContext from a source location.
    ///
    /// Looks up the source line and determines the appropriate comment style
    /// based on current JSX state.
    fn make_source_context(&self, loc: &Loc) -> SourceContext {
        let source_line = loc
            .file
            .get_line(loc.line - 1)
            .map(|cow| cow.to_string())
            .unwrap_or_default();
        let comment_style = self.decide_comment_style(&source_line, loc.line);

        SourceContext::new(
            SourceLocation::new(self.file_path, loc.line, loc.col_display + 1),
            source_line,
            comment_style,
        )
    }

    /// Record a raw translation call for later resolution in Phase 3.
    ///
    /// # Parameters
    ///
    /// - `loc`: Source location of the call
    /// - `translation_source`: Where the translation function came from (direct, props, fn params)
    /// - `argument`: The key argument (resolved, unresolved literal, template, etc.)
    /// - `call_kind`: Direct call (`t()`) or method call (`t.raw()`, `t.rich()`)
    fn collect_translation_call(
        &mut self,
        loc: Loc,
        translation_source: TranslationSource,
        argument: ValueSource,
        call_kind: TranslationCallKind,
    ) {
        let context = self.make_source_context(&loc);
        self.raw_calls.push(RawTranslationCall {
            context,
            translation_source,
            argument,
            call_kind,
        });
    }

    // ============================================================
    // Binding registration methods
    // ============================================================
    //
    // These methods detect and register translation function bindings
    // as we traverse the AST. They handle:
    // - Translation hooks: `const t = useTranslations("namespace")`
    // - Translation props: `function Component({ t }: { t: TranslationFunction })`
    // - Translation fn params: `getServerSideTranslations((t) => t("key"))`
    // - Shadowing detection: `const t = "something else"` shadows outer `t`

    /// Extract the object name from a computed member access expression.
    ///
    /// Examples:
    /// - `KEYS[index]` → Some("KEYS")
    /// - `obj[prop]` → Some("obj")
    fn extract_object_access_name(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Member(member) if member.prop.is_computed() => match &*member.obj {
                Expr::Ident(obj_ident) => Some(obj_ident.sym.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Register translation function bindings from function parameters.
    ///
    /// Looks for props destructuring patterns that match registered translation props
    /// from Phase 1. For example, if `ComponentA` is known to receive prop `tMessages`,
    /// then `function ComponentA({ tMessages })` will register `tMessages` as a translation binding.
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

    /// Register translation props from a destructuring pattern.
    ///
    /// Handles both:
    /// - `{ t }` → prop name and binding name are both "t"
    /// - `{ t: translate }` → prop name is "t", binding name is "translate"
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

    /// Extract the binding name from a pattern (identifier or assignment).
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

    /// Check if a prop matches a registered translation prop and register the binding if so.
    ///
    /// Looks up `{component_name}:{prop_name}` in the translation prop registry from Phase 1.
    /// If found, registers `binding_name` as a translation function binding with the prop's namespaces.
    fn try_register_translation_prop(
        &mut self,
        component_name: &str,
        prop_name: &str,
        binding_name: &str,
    ) {
        use crate::core::collect::make_translation_prop_key;

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

    /// Register translation function bindings from function parameters.
    ///
    /// Handles registration from Phase 1's translation function call registry.
    /// For example, if `getStaticProps` calls `getServerSideTranslations` passing
    /// its first parameter as `t`, we'll register that parameter as a translation binding.
    ///
    /// Also detects shadowing: if a parameter name matches an outer binding, mark it as shadowed.
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

    /// Extract a React component defined as an arrow function.
    ///
    /// Example: `const MyComponent = (props) => <div>{props.children}</div>`
    /// Returns: `Some(("MyComponent", arrow_expr))`
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
    ///
    /// Used for iterator detection to enable key resolution in patterns like:
    /// `KEYS.map(item => t(\`prefix.\${item}\`))` → registers "item" as iterator over "KEYS"
    fn extract_arrow_first_param(expr: &Expr) -> Option<String> {
        if let Expr::Arrow(arrow) = expr
            && let Some(first_param) = arrow.params.first()
            && let Pat::Ident(ident) = first_param
        {
            return Some(ident.id.sym.to_string());
        }
        None
    }

    /// Execute a closure with a statement context active for a specific line.
    ///
    /// This tracks that JSX on a given line is part of a specific kind of statement,
    /// which helps determine the correct comment style for hardcoded text issues.
    fn with_stmt_context<F>(&mut self, line: usize, kind: StmtKind, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.stmt_context.push(StmtContext { line, kind });
        f(self);
        self.stmt_context.pop();
    }

    /// Get the statement context for a specific line, if any.
    fn statement_context_for_line(&self, line: usize) -> Option<StmtKind> {
        self.stmt_context.last().and_then(|ctx| {
            if ctx.line == line {
                Some(ctx.kind)
            } else {
                None
            }
        })
    }

    /// Check if a statement kind should force JS comment style for a given line.
    ///
    /// Returns true if the line shows JavaScript syntax before the JSX, indicating
    /// the JSX is part of a JS statement (e.g., `return <div>` or `const x = <div>`).
    fn should_force_js_for_statement(&self, kind: StmtKind, trimmed_line: &str) -> bool {
        let lt_pos = trimmed_line.find('<');
        match kind {
            StmtKind::Return => trimmed_line.starts_with("return "),
            StmtKind::VarInit => {
                if trimmed_line.starts_with("const ")
                    || trimmed_line.starts_with("let ")
                    || trimmed_line.starts_with("var ")
                {
                    return true;
                }
                let eq_pos = trimmed_line.find('=');
                matches!((eq_pos, lt_pos), (Some(eq), Some(lt)) if eq < lt)
            }
            StmtKind::ArrowExpr => {
                let arrow_pos = trimmed_line.find("=>");
                matches!((arrow_pos, lt_pos), (Some(arrow), Some(lt)) if arrow < lt)
            }
        }
    }
}

impl<'a> Visit for FileAnalyzer<'a> {
    fn visit_return_stmt(&mut self, node: &ReturnStmt) {
        if let Some(arg) = &node.arg {
            let line = self.source_map.lookup_char_pos(node.span.lo).line;
            self.with_stmt_context(line, StmtKind::Return, |this| {
                arg.visit_with(this);
            });
        }
    }

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
        let expr_line = self.source_map.lookup_char_pos(node.span.lo).line;
        self.jsx_state.in_expr = true;
        self.jsx_state.expr_start_line = expr_line;

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
        for param in &node.params {
            param.visit_with(self);
        }
        match &*node.body {
            BlockStmtOrExpr::Expr(expr) => {
                let line = self.source_map.lookup_char_pos(expr.span().lo).line;
                self.with_stmt_context(line, StmtKind::ArrowExpr, |this| {
                    expr.visit_with(this);
                });
            }
            BlockStmtOrExpr::BlockStmt(block) => {
                block.visit_with(self);
            }
        }
        self.binding_context.exit_scope();
    }

    fn visit_var_declarator(&mut self, node: &VarDeclarator) {
        node.name.visit_with(self);
        if let Some(init) = &node.init {
            let line = self.source_map.lookup_char_pos(init.span().lo).line;
            self.with_stmt_context(line, StmtKind::VarInit, |this| {
                init.visit_with(this);
            });
        }
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
        // Handle direct translation calls: t("key")
        if let Callee::Expr(expr) = &node.callee
            && let Expr::Ident(ident) = &**expr
        {
            let fn_name = ident.sym.as_str();

            if let Some(translation_source) = self.binding_context.get_binding(fn_name).cloned()
                && !translation_source.is_shadowed()
            {
                let loc = self.source_map.lookup_char_pos(node.span.lo);

                if let Some(arg) = node.args.first() {
                    let argument = self.value_analyzer.analyze_expr(&arg.expr);
                    self.collect_translation_call(
                        loc,
                        translation_source,
                        argument,
                        TranslationCallKind::Direct,
                    );
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
                let argument = self.value_analyzer.analyze_expr(&arg.expr);
                self.collect_translation_call(
                    loc,
                    translation_source,
                    argument,
                    TranslationCallKind::Method(method_name.to_string()),
                );
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

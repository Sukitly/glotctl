use std::collections::HashSet;

use swc_common::{Loc, SourceMap, comments::SingleThreadedComments};
use swc_ecma_ast::{
    BinaryOp, Expr, JSXAttr, JSXAttrName, JSXAttrValue, JSXElement, JSXElementName, JSXExpr,
    JSXExprContainer, JSXFragment, JSXText, Lit, Module,
};
use swc_ecma_visit::{Visit, VisitWith};

#[derive(Debug, Clone)]
pub struct HardcodedIssue {
    pub file_path: String,
    pub line: usize,
    pub col: usize,
    pub text: String,
    pub source_line: String,
    pub in_jsx_context: bool,
}

enum Directive {
    Disable,
    Enable,
    DisableNextLine,
}

impl Directive {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "glot-disable" => Some(Self::Disable),
            "glot-enable" => Some(Self::Enable),
            "glot-disable-next-line" => Some(Self::DisableNextLine),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DisabledRange {
    start: usize,
    end: usize,
}

impl DisabledRange {
    fn new(start: usize, end: Option<usize>) -> Self {
        Self {
            start,
            end: end.unwrap_or(usize::MAX),
        }
    }
    fn single_line(line: usize) -> Self {
        Self {
            start: line,
            end: line,
        }
    }
}

#[derive(Debug, Default)]
struct IgnoreContext {
    disabled_ranges: Vec<DisabledRange>,
}

impl IgnoreContext {
    fn should_ignore(&self, line: usize) -> bool {
        self.disabled_ranges
            .iter()
            .any(|r| (r.start..=r.end).contains(&line))
    }

    fn add_disabled_range(&mut self, start: usize, end: Option<usize>) {
        self.disabled_ranges.push(DisabledRange::new(start, end));
    }

    fn add_disabled_line(&mut self, line: usize) {
        self.disabled_ranges.push(DisabledRange::single_line(line));
    }

    fn build(comments: &SingleThreadedComments, source_map: &SourceMap) -> Self {
        let mut ctx = IgnoreContext::default();
        let (leading, trailing) = comments.borrow_all();

        let mut all_comments: Vec<_> = leading
            .iter()
            .chain(trailing.iter())
            .flat_map(|(_, cmts)| cmts.iter())
            .collect();
        all_comments.sort_by_key(|cmt| source_map.lookup_char_pos(cmt.span.lo).line);

        let mut current_disable_line: Option<usize> = None;

        for cmt in all_comments {
            let text = cmt.text.trim();
            let loc = source_map.lookup_char_pos(cmt.span.lo);

            if let Some(directive) = Directive::parse(text) {
                match directive {
                    Directive::Disable => {
                        // Only start a new range if not already in a disabled state
                        // Consecutive disables are ignored - existing range continues
                        if current_disable_line.is_none() {
                            current_disable_line = Some(loc.line);
                        }
                    }
                    Directive::Enable => {
                        if let Some(start) = current_disable_line {
                            let end = loc.line.saturating_sub(1);
                            ctx.add_disabled_range(start, Some(end));
                            current_disable_line = None;
                        }
                    }
                    Directive::DisableNextLine => {
                        let next_line = loc.line + 1;
                        ctx.add_disabled_line(next_line);
                    }
                }
            }
        }
        if let Some(start) = current_disable_line {
            ctx.add_disabled_range(start, None);
        }

        ctx
    }
}

/// Checks if the text contains at least one Unicode alphabetic character.
/// Returns false for empty strings, pure numbers, or pure symbols.
///
/// Examples:
/// - "Hello" -> true (contains letters)
/// - "Hello123" -> true (contains letters)
/// - "123" -> false (pure numbers)
/// - "---" -> false (pure symbols)
/// - "$100" -> false (no letters)
pub fn contains_alphabetic(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    text.chars().any(|c| c.is_alphabetic())
}

/// Tracks JSX context state during AST traversal.
///
/// These flags are independent (not mutually exclusive):
/// - `in_context`: Inside JSX element/fragment children
/// - `in_attr`: Inside a JSX attribute
/// - `in_expr`: Inside a JSX expression container {}
/// - `in_element_expr`: Current JSX element is inside an expression (ternary, &&, etc.)
#[derive(Debug, Clone, Copy, Default)]
struct JsxState {
    in_context: bool,
    in_attr: bool,
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
            in_expr: false,
            in_element_expr: self.in_expr, // if entering from expr, element is in expr
        }
    }
}

pub struct HardcodedChecker<'a> {
    file_path: &'a str,
    checked_attributes: &'a [String],
    ignore_texts: &'a HashSet<String>,
    source_map: &'a SourceMap,
    ignore_context: IgnoreContext,
    pub issues: Vec<HardcodedIssue>,
    jsx_state: JsxState,
}

impl<'a> HardcodedChecker<'a> {
    pub fn new(
        file_path: &'a str,
        checked_attributes: &'a [String],
        ignore_texts: &'a HashSet<String>,
        source_map: &'a SourceMap,
        comments: &SingleThreadedComments,
    ) -> Self {
        let ignore_context = IgnoreContext::build(comments, source_map);
        Self {
            file_path,
            checked_attributes,
            ignore_texts,
            source_map,
            ignore_context,
            issues: Vec::new(),
            jsx_state: JsxState::default(),
        }
    }

    fn should_report(&self, line: usize, text: &str) -> bool {
        if self.ignore_context.should_ignore(line) {
            return false;
        }
        let text = text.trim();
        if self.ignore_texts.contains(text) {
            return false;
        }
        contains_alphabetic(text)
    }

    /// Determines whether to use JSX comment style `{/* */}` or JS comment style `//`.
    ///
    /// The decision depends on where the comment will be inserted:
    /// - Attribute on separate line (multi-line format): `//` comment in attribute list
    /// - Attribute on same line as element: `{/* */}` in JSX children (if in JSX context)
    /// - Text on same line as element: comment goes BEFORE element
    ///   - If element is inside expression (ternary, map, etc.): `//`
    ///   - Otherwise: `{/* */}`
    /// - Text on its own line: `{/* */}` inside parent's children
    fn should_use_jsx_comment(&self, source_line: &str) -> bool {
        let trimmed_line = source_line.trim_start();
        let line_starts_with_element = trimmed_line.starts_with('<');
        let state = &self.jsx_state;

        if state.in_attr {
            // Attribute: {/* */} only if element starts on this line AND in pure JSX children
            line_starts_with_element && state.in_context && !state.in_expr
        } else if line_starts_with_element {
            // Text on same line as element tag - comment goes BEFORE element
            // If the element is inside an expression (e.g. ternary branch),
            // we must use // comments even if we are inside a JSX structure.
            if state.in_element_expr {
                false
            } else {
                state.in_context && !state.in_expr
            }
        } else {
            // Text on its own line - comment goes INSIDE parent's children
            state.in_context
        }
    }

    fn add_issue(&mut self, value: &str, loc: Loc) {
        let source_line = loc
            .file
            .get_line(loc.line - 1)
            .map(|cow| cow.to_string())
            .unwrap_or_default();

        let use_jsx_comment = self.should_use_jsx_comment(&source_line);

        self.issues.push(HardcodedIssue {
            file_path: self.file_path.to_owned(),
            line: loc.line,
            col: loc.col_display + 1,
            text: value.to_owned(),
            source_line,
            in_jsx_context: use_jsx_comment,
        });
    }

    fn check_line(&mut self, value: &str, loc: Loc) {
        if self.should_report(loc.line, value) {
            self.add_issue(value, loc);
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Lit(Lit::Str(s)) => {
                if let Some(value) = s.value.as_str() {
                    let loc = self.source_map.lookup_char_pos(s.span.lo);
                    self.check_line(value, loc);
                };
            }
            Expr::Tpl(tpl) => {
                for quasi in &tpl.quasis {
                    if let Some(cooked) = &quasi.cooked
                        && let Some(value) = cooked.as_str()
                    {
                        let loc = self.source_map.lookup_char_pos(quasi.span.lo);
                        self.check_line(value, loc);
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

    pub fn check(mut self, module: &Module) -> Vec<HardcodedIssue> {
        self.visit_module(module);
        self.issues
    }
}

impl<'a> Visit for HardcodedChecker<'a> {
    fn visit_jsx_element(&mut self, node: &JSXElement) {
        // Visit opening element (attributes) - keep current state
        // so nested elements inherit parent's context
        node.opening.visit_with(self);

        // Check if this is a <style> tag
        let is_style = if let JSXElementName::Ident(ident) = &node.opening.name {
            ident.sym == "style"
        } else {
            false
        };

        if is_style {
            // Visit closing element (if any) and return, skipping children
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

        // Visit closing element (if any)
        if let Some(closing) = &node.closing {
            closing.visit_with(self);
        }
    }

    fn visit_jsx_fragment(&mut self, node: &JSXFragment) {
        // Visit opening
        node.opening.visit_with(self);

        // Visit children with JSX context state
        let prev_state = self.jsx_state;
        self.jsx_state = prev_state.for_children();

        for child in &node.children {
            child.visit_with(self);
        }
        self.jsx_state = prev_state;

        // Visit closing
        node.closing.visit_with(self);
    }

    fn visit_jsx_text(&mut self, node: &JSXText) {
        let raw_value = &node.value;
        let trimmed = raw_value.trim();

        if trimmed.is_empty() {
            return;
        }

        // Calculate byte offset of trimmed text by measuring leading whitespace length.
        // This is more reliable than using find() which could theoretically match
        // the wrong occurrence in edge cases.
        let trim_start_offset = raw_value.len() - raw_value.trim_start().len();

        // Calculate the actual position by offsetting from span.lo
        let actual_pos = node.span.lo + swc_common::BytePos(trim_start_offset as u32);
        let loc = self.source_map.lookup_char_pos(actual_pos);

        self.check_line(trimmed, loc);
    }

    fn visit_jsx_expr_container(&mut self, node: &JSXExprContainer) {
        // Mark that we're inside a JSX expression container {}
        // Code inside {} is JS context, not JSX children
        let prev_state = self.jsx_state;
        self.jsx_state.in_expr = true;

        if let JSXExpr::Expr(expr) = &node.expr {
            self.check_expr(expr);
        }
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
        if !self.checked_attributes.contains(&attr_name) {
            return;
        }

        // Mark that we're in an attribute context
        let prev_state = self.jsx_state;
        self.jsx_state.in_attr = true;

        if let Some(JSXAttrValue::Str(s)) = &node.value
            && let Some(value) = s.value.as_str()
        {
            let loc = self.source_map.lookup_char_pos(s.span.lo);
            self.check_line(value, loc);
        }

        self.jsx_state = prev_state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_alphabetic() {
        assert!(contains_alphabetic("Hello"));
        assert!(contains_alphabetic("你好"));
        assert!(contains_alphabetic("Hello123"));
        assert!(!contains_alphabetic("123"));
        assert!(!contains_alphabetic("---"));
        assert!(!contains_alphabetic("$100"));
    }

    #[test]
    fn test_consecutive_disable_ignored() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

        let cm = SourceMap::new(FilePathMapping::empty());
        // Test: consecutive disables should be ignored (only one range created)
        let code = r#"
function App() {
    return (
        <div>
            {/* glot-disable */}
            <span>Hello</span>
            {/* glot-disable */}
            <span>World</span>
            {/* glot-enable */}
            <span>Visible</span>
        </div>
    );
}
"#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());
        let comments = SingleThreadedComments::default();

        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            Some(&comments),
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().expect("parse failed");

        let ignored = HashSet::new();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        // Only "Visible" should be detected (Hello and World are in disabled range)
        assert_eq!(issues.len(), 1, "Expected 1 issue, got {:?}", issues);
        assert_eq!(issues[0].text, "Visible");
    }

    #[test]
    fn test_multiline_jsx_text_reports_correct_line() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

        let cm = SourceMap::new(FilePathMapping::empty());
        // Test: multiline JSX text should report the line where text actually starts
        // Line 1: function App() {
        // Line 2:     return (
        // Line 3:         <div>
        // Line 4:             (empty/whitespace)
        // Line 5:             Hello World  <- text is here
        // Line 6:             (empty/whitespace)
        // Line 7:         </div>
        // Line 8:     );
        // Line 9: }
        let code = "function App() {\n    return (\n        <div>\n\n            Hello World\n\n        </div>\n    );\n}";

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());
        let comments = SingleThreadedComments::default();

        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            Some(&comments),
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().expect("parse failed");

        let ignored = HashSet::new();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        assert_eq!(issues.len(), 1, "Expected 1 issue, got {:?}", issues);
        assert_eq!(issues[0].text, "Hello World");
        // The text "Hello World" is on line 5, not line 3 (where <div> starts)
        assert_eq!(
            issues[0].line, 5,
            "Expected line 5 for 'Hello World', got line {}",
            issues[0].line
        );
    }

    #[test]
    fn test_nested_jsx_expr_comment_style() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

        let cm = SourceMap::new(FilePathMapping::empty());
        let code = r#"
            function App() {
                return (
                    <div>
                        {true && (
                            <span id="test">
                                Hardcoded
                            </span>
                        )}
                    </div>
                );
            }
        "#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());

        let lexer = swc_ecma_parser::lexer::Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            None,
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().unwrap();
        let comments = SingleThreadedComments::default();

        let ignored = HashSet::new();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        assert_eq!(issues.len(), 1);
        // We expect in_jsx_context to be true (it is inside span which is inside div)
        // AND it should use JSX comment style because it is a child of span
        assert!(
            issues[0].in_jsx_context,
            "Should use JSX comment style {{/* */}}"
        );
    }

    #[test]
    fn test_ternary_jsx_expr_comment_style() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

        let cm = SourceMap::new(FilePathMapping::empty());
        let code = r#"
            function App() {
                return (
                    <div>
                        {true ? (
                            <p>No console output</p>
                        ) : null}
                    </div>
                );
            }
        "#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());

        let lexer = swc_ecma_parser::lexer::Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            None,
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().unwrap();
        let comments = SingleThreadedComments::default();

        let ignored = HashSet::new();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        assert_eq!(issues.len(), 1);

        // Logic:
        // <p>No console output</p>
        // line_starts_with_element = true ("                        <p>No console output</p>")
        // It is inside a ternary expression branch.
        // So the comment should be placed before <p>, inside the parentheses of the ternary.
        // It must be // or /* */, NOT {/* */}

        assert!(
            !issues[0].in_jsx_context,
            "Should NOT use JSX comment style {{/* */}} inside ternary expression"
        );
    }

    #[test]
    fn test_ignore_style_tag_children() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

        let cm = SourceMap::new(FilePathMapping::empty());
        let code = r#"
            function App() {
                return (
                    <svg>
                        <style>{`
                            .class { color: red; }
                        `}</style>
                        <title>Loading...</title>
                    </svg>
                );
            }
        "#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());

        let lexer = swc_ecma_parser::lexer::Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            None,
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().unwrap();
        let comments = SingleThreadedComments::default();

        let ignored = HashSet::new();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        // Should only detect "Loading...", ignoring the CSS in style tag
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Loading...");
    }

    #[test]
    fn test_logical_or_expression() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

        let cm = SourceMap::new(FilePathMapping::empty());
        let code = r#"
            function App() {
                const error = null;
                return (
                    <div>
                        {error || "Default message"}
                    </div>
                );
            }
        "#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());

        let lexer = swc_ecma_parser::lexer::Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            None,
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().unwrap();
        let comments = SingleThreadedComments::default();

        let ignored = HashSet::new();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        // Should detect the string in the || expression
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Default message");
    }

    #[test]
    fn test_attribute_detection() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

        let cm = SourceMap::new(FilePathMapping::empty());
        let code = r#"
function App() {
    return (
        <div>
            <input placeholder="Enter name" />
            <img alt="Profile picture" />
            <button title="Click me">OK</button>
        </div>
    );
}
"#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());
        let comments = SingleThreadedComments::default();

        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            Some(&comments),
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().expect("parse failed");

        let ignored = HashSet::new();
        let attrs = vec![
            "placeholder".to_string(),
            "alt".to_string(),
            "title".to_string(),
        ];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        // Should detect: "OK" (JSX text), "Enter name", "Profile picture", "Click me"
        assert_eq!(issues.len(), 4, "Expected 4 issues, got {:?}", issues);
        assert!(issues.iter().any(|i| i.text == "Enter name"));
        assert!(issues.iter().any(|i| i.text == "Profile picture"));
        assert!(issues.iter().any(|i| i.text == "Click me"));
        assert!(issues.iter().any(|i| i.text == "OK"));
    }

    #[test]
    fn test_whitespace_only_text_ignored() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

        let cm = SourceMap::new(FilePathMapping::empty());
        // Test that whitespace-only JSX text is ignored
        let code = r#"
function App() {
    return (
        <div>
            {/* lots of whitespace between elements */}
            <span>Hello</span>

            <span>World</span>
        </div>
    );
}
"#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());
        let comments = SingleThreadedComments::default();

        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            Some(&comments),
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().expect("parse failed");

        let ignored = HashSet::new();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        // Should only detect "Hello" and "World", not whitespace
        assert_eq!(issues.len(), 2, "Expected 2 issues, got {:?}", issues);
        assert!(issues.iter().any(|i| i.text == "Hello"));
        assert!(issues.iter().any(|i| i.text == "World"));
    }

    #[test]
    fn test_ignore_texts_functionality() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

        let cm = SourceMap::new(FilePathMapping::empty());
        let code = r#"
function App() {
    return (
        <div>
            <span>Hello</span>
            <span>World</span>
            <span>OK</span>
        </div>
    );
}
"#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());
        let comments = SingleThreadedComments::default();

        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            Some(&comments),
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().expect("parse failed");

        // Ignore "OK" and "Hello"
        let ignored: HashSet<String> = ["OK", "Hello"].iter().map(|s| s.to_string()).collect();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        // Should only detect "World"
        assert_eq!(issues.len(), 1, "Expected 1 issue, got {:?}", issues);
        assert_eq!(issues[0].text, "World");
    }

    #[test]
    fn test_nested_jsx_elements() {
        use swc_common::{FileName, FilePathMapping};
        use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

        let cm = SourceMap::new(FilePathMapping::empty());
        let code = r#"
function App() {
    return (
        <div>
            <section>
                <article>
                    <p>Deeply nested text</p>
                </article>
            </section>
        </div>
    );
}
"#;

        let fm = cm.new_source_file(FileName::Anon.into(), code.to_string());
        let comments = SingleThreadedComments::default();

        let lexer = Lexer::new(
            Syntax::Typescript(TsSyntax {
                tsx: true,
                ..Default::default()
            }),
            Default::default(),
            StringInput::from(&*fm),
            Some(&comments),
        );

        let mut parser = Parser::new_from(lexer);
        let module = parser.parse_module().expect("parse failed");

        let ignored = HashSet::new();
        let attrs = vec![];
        let checker = HardcodedChecker::new("test.tsx", &attrs, &ignored, &cm, &comments);
        let issues = checker.check(&module);

        assert_eq!(issues.len(), 1, "Expected 1 issue, got {:?}", issues);
        assert_eq!(issues[0].text, "Deeply nested text");
    }
}

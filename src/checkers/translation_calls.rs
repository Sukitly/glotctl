//! Finds lines that contain translation function calls.
//!
//! Used by the baseline command to skip inserting comments on lines
//! that already have translation calls (to avoid marking keys as unused).

use std::collections::HashSet;

use swc_common::SourceMap;
use swc_ecma_ast::{CallExpr, Callee, Expr, Module, Pat, VarDecl};
use swc_ecma_visit::{Visit, VisitWith};

use super::is_translation_hook;

/// Finds all lines that contain translation function calls.
pub struct TranslationCallFinder<'a> {
    source_map: &'a SourceMap,
    /// Variable names that are bound to translation functions
    bindings: HashSet<String>,
    /// Line numbers that have translation calls
    pub lines_with_calls: HashSet<usize>,
}

impl<'a> TranslationCallFinder<'a> {
    pub fn new(source_map: &'a SourceMap) -> Self {
        Self {
            source_map,
            bindings: HashSet::new(),
            lines_with_calls: HashSet::new(),
        }
    }

    /// Analyze a module and return the set of lines with translation calls.
    pub fn find(mut self, module: &Module) -> HashSet<usize> {
        module.visit_with(&mut self);
        self.lines_with_calls
    }
}

impl<'a> Visit for TranslationCallFinder<'a> {
    fn visit_var_decl(&mut self, node: &VarDecl) {
        for decl in &node.decls {
            if let Some(init) = &decl.init {
                // Handle both sync and async: useTranslations() and await getTranslations()
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
                        self.bindings.insert(var_name);
                    }
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
            if self.bindings.contains(fn_name) {
                let loc = self.source_map.lookup_char_pos(node.span.lo);
                self.lines_with_calls.insert(loc.line);
            }
        }
        node.visit_children_with(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_common::{FileName, FilePathMapping};
    use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

    fn find_translation_lines(code: &str) -> HashSet<usize> {
        let cm = SourceMap::new(FilePathMapping::empty());
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

        TranslationCallFinder::new(&cm).find(&module)
    }

    #[test]
    fn test_finds_standard_t_call() {
        let code = r#"
            import { useTranslations } from "next-intl";
            function App() {
                const t = useTranslations("Common");
                return <div>{t("hello")}</div>;
            }
        "#;
        let lines = find_translation_lines(code);
        assert!(lines.contains(&5), "Should find t() call on line 5");
    }

    #[test]
    fn test_finds_renamed_translation_function() {
        let code = r#"
            import { useTranslations } from "next-intl";
            function App() {
                const translate = useTranslations("Common");
                return <div>{translate("hello")}</div>;
            }
        "#;
        let lines = find_translation_lines(code);
        assert!(lines.contains(&5), "Should find translate() call on line 5");
    }

    #[test]
    fn test_finds_async_get_translations() {
        let code = r#"
            import { getTranslations } from "next-intl/server";
            async function Page() {
                const t = await getTranslations("Common");
                return <div>{t("hello")}</div>;
            }
        "#;
        let lines = find_translation_lines(code);
        assert!(lines.contains(&5), "Should find t() call on line 5");
    }

    #[test]
    fn test_ignores_unrelated_function_calls() {
        let code = r#"
            function App() {
                const t = (x) => x;
                return <div>{t("hello")}</div>;
            }
        "#;
        let lines = find_translation_lines(code);
        assert!(
            lines.is_empty(),
            "Should not find any translation calls: {:?}",
            lines
        );
    }

    #[test]
    fn test_multiple_calls_same_line() {
        let code = r#"
            import { useTranslations } from "next-intl";
            function App() {
                const t = useTranslations("Common");
                return <div>{t("hello")} {t("world")}</div>;
            }
        "#;
        let lines = find_translation_lines(code);
        assert_eq!(lines.len(), 1, "Should deduplicate same line");
        assert!(lines.contains(&5));
    }
}

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use swc_common::{
    BytePos, FileName, Globals, SourceMap,
    comments::{Comment, SingleThreadedComments},
};
use swc_ecma_ast::Module;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

/// Map of byte positions to comments.
pub type CommentMap = HashMap<BytePos, Vec<Comment>>;

/// A parsed comment that is tracked independently from SWC comment storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedComment {
    pub line: usize,
    pub text: String,
}

/// Thread-safe extracted comments from SingleThreadedComments.
/// Extracted during parsing and stored independently of swc types.
#[derive(Debug, Clone)]
pub struct ExtractedComments {
    pub leading: CommentMap,
    pub trailing: CommentMap,
    pub detached: Vec<DetachedComment>,
}

impl ExtractedComments {
    /// Extract comments from SingleThreadedComments.
    /// This must be called before SingleThreadedComments is dropped.
    pub fn from_swc(comments: &SingleThreadedComments) -> Self {
        let (leading, trailing) = comments.borrow_all();
        Self {
            leading: leading.iter().map(|(k, v)| (*k, v.clone())).collect(),
            trailing: trailing.iter().map(|(k, v)| (*k, v.clone())).collect(),
            detached: Vec::new(),
        }
    }

    /// Provide an interface compatible with SingleThreadedComments for CommentCollector.
    /// Returns references to avoid cloning the entire HashMap.
    pub fn borrow_all(&self) -> (&CommentMap, &CommentMap) {
        (&self.leading, &self.trailing)
    }
}

pub struct ParsedJSX {
    pub module: Module,
    pub source_map: Arc<SourceMap>,
    pub comments: ExtractedComments,
    pub astro_template_start_line: Option<usize>,
}

pub(crate) fn parse_module_source(
    code: String,
    file_path: &str,
    source_map: Arc<SourceMap>,
    tsx: bool,
    source_kind: &str,
) -> Result<ParsedJSX> {
    use swc_common::GLOBALS;

    GLOBALS.set(&Globals::new(), || {
        let source_file = source_map.new_source_file(FileName::Real(file_path.into()).into(), code);

        let syntax = Syntax::Typescript(TsSyntax {
            tsx,
            ..Default::default()
        });

        let comments = SingleThreadedComments::default();
        let mut parser = Parser::new(syntax, StringInput::from(&*source_file), Some(&comments));

        let module = parser
            .parse_module()
            .map_err(|e| anyhow!("Failed to parse {} string: {:?}", source_kind, e))?;

        let extracted_comments = ExtractedComments::from_swc(&comments);

        Ok(ParsedJSX {
            module,
            source_map,
            comments: extracted_comments,
            astro_template_start_line: None,
        })
    })
}

/// Parse JSX/TSX source code string into an AST.
///
/// This is the core parsing function. For file-based parsing with caching,
/// use `CheckContext::ensure_parsed_files()` instead.
///
/// Accepts a shared SourceMap for thread-safe parallel parsing.
pub fn parse_jsx_source(
    code: String,
    file_path: &str,
    source_map: Arc<SourceMap>,
) -> Result<ParsedJSX> {
    let is_tsx = file_path.ends_with(".tsx") || file_path.ends_with(".jsx");
    parse_module_source(code, file_path, source_map, is_tsx, "tsx")
}

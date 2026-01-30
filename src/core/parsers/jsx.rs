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

/// Thread-safe extracted comments from SingleThreadedComments.
/// Extracted during parsing and stored independently of swc types.
#[derive(Debug, Clone)]
pub struct ExtractedComments {
    pub leading: CommentMap,
    pub trailing: CommentMap,
}

impl ExtractedComments {
    /// Extract comments from SingleThreadedComments.
    /// This must be called before SingleThreadedComments is dropped.
    pub fn from_swc(comments: &SingleThreadedComments) -> Self {
        let (leading, trailing) = comments.borrow_all();
        Self {
            leading: leading.iter().map(|(k, v)| (*k, v.clone())).collect(),
            trailing: trailing.iter().map(|(k, v)| (*k, v.clone())).collect(),
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
    use swc_common::GLOBALS;

    // Wrap in GLOBALS.set() for thread safety
    GLOBALS.set(&Globals::new(), || {
        let source_file = source_map.new_source_file(FileName::Real(file_path.into()).into(), code);

        let syntax = Syntax::Typescript(TsSyntax {
            tsx: true,
            ..Default::default()
        });

        let comments = SingleThreadedComments::default();
        let mut parser = Parser::new(syntax, StringInput::from(&*source_file), Some(&comments));

        let module = parser
            .parse_module()
            .map_err(|e| anyhow!("Failed to parse tsx string: {:?}", e))?;

        // Extract comments immediately (before SingleThreadedComments drops)
        let extracted_comments = ExtractedComments::from_swc(&comments);

        Ok(ParsedJSX {
            module,
            source_map,
            comments: extracted_comments,
        })
    })
}

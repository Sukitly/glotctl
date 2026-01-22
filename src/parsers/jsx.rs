use anyhow::{Result, anyhow};
use swc_common::{FileName, SourceMap, comments::SingleThreadedComments};
use swc_ecma_ast::Module;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

pub struct ParsedJSX {
    pub module: Module,
    pub source_map: SourceMap,
    pub comments: SingleThreadedComments,
    pub source: String,
}

/// Parse JSX/TSX source code string into an AST.
///
/// This is the core parsing function. For file-based parsing with caching,
/// use `CheckContext::ensure_parsed_files()` instead.
pub fn parse_jsx_source(code: String, file_path: &str) -> Result<ParsedJSX> {
    let source_map = SourceMap::default();
    let source_file =
        source_map.new_source_file(FileName::Real(file_path.into()).into(), code.clone());

    let syntax = Syntax::Typescript(TsSyntax {
        tsx: true,
        ..Default::default()
    });
    let comments = SingleThreadedComments::default();
    let mut parser = Parser::new(syntax, StringInput::from(&*source_file), Some(&comments));
    let module = parser
        .parse_module()
        .map_err(|e| anyhow!("Failed to parse tsx string: {:?}", e))?;
    Ok(ParsedJSX {
        module,
        source_map,
        comments,
        source: code,
    })
}

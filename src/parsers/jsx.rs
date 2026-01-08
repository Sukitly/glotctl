use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow};
use swc_common::{FileName, SourceMap, comments::SingleThreadedComments};
use swc_ecma_ast::Module;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};

pub struct ParsedJSX {
    pub module: Module,
    pub source_map: SourceMap,
    pub comments: SingleThreadedComments,
    pub source: String,
}

fn parse_jsx(code: String, file_path: &str) -> Result<ParsedJSX> {
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

pub fn parse_jsx_file(file_path: impl AsRef<Path>) -> Result<ParsedJSX> {
    let file_path: &Path = file_path.as_ref();
    let code = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {:?}", file_path))?;
    let file_path = file_path
        .as_os_str()
        .to_str()
        .with_context(|| format!("Invalid file path: {:?}", file_path))?;
    parse_jsx(code, file_path)
}

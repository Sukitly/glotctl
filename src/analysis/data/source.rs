use crate::analysis::data::comment_style::CommentStyle;

/// Pure position information in source code files (TSX/JSX/TS/JS).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceLocation {
    pub file_path: String,
    pub line: usize,
    pub col: usize,
}

impl SourceLocation {
    pub fn new(file_path: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            file_path: file_path.into(),
            line,
            col,
        }
    }
}

/// Position with context information in source code files.
///
/// Contains everything needed for:
/// - Reporter: to display the issue location and source context
/// - Action: to insert comments (knows whether to use JSX or JS comment style)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContext {
    pub location: SourceLocation,
    /// The source code line content for display.
    pub source_line: String,
    /// Comment style to use when inserting comments at this location.
    /// Pre-computed based on AST context and source line analysis.
    pub comment_style: CommentStyle,
}

impl SourceContext {
    pub fn new(
        location: SourceLocation,
        source_line: impl Into<String>,
        comment_style: CommentStyle,
    ) -> Self {
        Self {
            location,
            source_line: source_line.into(),
            comment_style,
        }
    }

    // Convenience accessors
    pub fn file_path(&self) -> &str {
        &self.location.file_path
    }

    pub fn line(&self) -> usize {
        self.location.line
    }

    pub fn col(&self) -> usize {
        self.location.col
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::{CommentStyle, SourceContext, SourceLocation};

    #[test]
    fn test_source_location_new() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        assert_eq!(loc.file_path, "./src/app.tsx");
        assert_eq!(loc.line, 10);
        assert_eq!(loc.col, 5);
    }

    #[test]
    fn test_source_context_new() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        let ctx = SourceContext::new(loc, "const x = \"Hello\";", CommentStyle::Js);
        assert_eq!(ctx.file_path(), "./src/app.tsx");
        assert_eq!(ctx.line(), 10);
        assert_eq!(ctx.col(), 5);
        assert_eq!(ctx.source_line, "const x = \"Hello\";");
        assert_eq!(ctx.comment_style, CommentStyle::Js);
    }

    #[test]
    fn test_source_context_jsx_comment() {
        let loc = SourceLocation::new("./src/app.tsx", 20, 3);
        let ctx = SourceContext::new(loc, "<span>{t(`key`)}</span>", CommentStyle::Jsx);
        assert_eq!(ctx.comment_style, CommentStyle::Jsx);
        assert!(ctx.comment_style.is_jsx());
    }
}

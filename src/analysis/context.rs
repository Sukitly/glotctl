//! Location and Context types for i18n analysis.
//!
//! This module defines the fundamental location and context types:
//! - `SourceLocation` / `SourceContext`: For source code (TSX/JSX) positions
//! - `MessageLocation` / `MessageContext`: For message file (JSON) positions
//! - `LocaleTypeMismatch`: For type mismatch information in a specific locale

use std::fmt;
/// Type of JSON value at a key path.
///
/// Used to detect type mismatches between primary and replica locales.
/// For example, if primary has an array but replica has a string, this
/// causes runtime crashes when the app tries to iterate over the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValueType {
    /// A simple string value
    String,
    /// A string array (accessed via t.raw() as a whole)
    StringArray,
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueType::String => write!(f, "string"),
            ValueType::StringArray => write!(f, "array"),
        }
    }
}

// ============================================================
// Comment Style
// ============================================================

/// Comment style to use when inserting comments in source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    /// JavaScript comment: `// comment`
    Js,
    /// JSX comment: `{/* comment */}`
    Jsx,
}

impl CommentStyle {
    /// Returns true if this is JSX comment style.
    pub fn is_jsx(&self) -> bool {
        matches!(self, CommentStyle::Jsx)
    }
}

// ============================================================
// Source Location & Context (for TSX/JSX files)
// ============================================================

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

// ============================================================
// Message Location & Context (for JSON files)
// ============================================================

/// Pure position information in message/locale files (JSON).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageLocation {
    pub file_path: String,
    pub line: usize,
    pub col: usize,
}

impl MessageLocation {
    pub fn new(file_path: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            file_path: file_path.into(),
            line,
            col,
        }
    }

    /// Create with default column (1).
    pub fn with_line(file_path: impl Into<String>, line: usize) -> Self {
        Self {
            file_path: file_path.into(),
            line,
            col: 1,
        }
    }
}

/// Position with context information in message files.
///
/// Contains the key and value at this location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageContext {
    pub location: MessageLocation,
    /// The translation key (e.g., "Common.submit").
    pub key: String,
    /// The translation value.
    pub value: String,
}

impl MessageContext {
    pub fn new(
        location: MessageLocation,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            location,
            key: key.into(),
            value: value.into(),
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

// ============================================================
// Locale Type Mismatch (for TypeMismatchIssue)
// ============================================================

/// Information about a locale with mismatched value type.
///
/// Used in `TypeMismatchIssue` to describe which locales have
/// different types than the primary locale.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocaleTypeMismatch {
    pub locale: String,
    pub actual_type: ValueType,
    pub location: MessageLocation,
}

impl LocaleTypeMismatch {
    pub fn new(
        locale: impl Into<String>,
        actual_type: ValueType,
        location: MessageLocation,
    ) -> Self {
        Self {
            locale: locale.into(),
            actual_type,
            location,
        }
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use crate::analysis::context::*;

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

    #[test]
    fn test_comment_style_is_jsx() {
        assert!(!CommentStyle::Js.is_jsx());
        assert!(CommentStyle::Jsx.is_jsx());
    }

    #[test]
    fn test_message_location_new() {
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        assert_eq!(loc.file_path, "./messages/en.json");
        assert_eq!(loc.line, 5);
        assert_eq!(loc.col, 3);
    }

    #[test]
    fn test_message_location_with_line() {
        let loc = MessageLocation::with_line("./messages/en.json", 5);
        assert_eq!(loc.file_path, "./messages/en.json");
        assert_eq!(loc.line, 5);
        assert_eq!(loc.col, 1);
    }

    #[test]
    fn test_message_context_new() {
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Common.submit", "Submit");
        assert_eq!(ctx.file_path(), "./messages/en.json");
        assert_eq!(ctx.line(), 5);
        assert_eq!(ctx.key, "Common.submit");
        assert_eq!(ctx.value, "Submit");
    }

    #[test]
    fn test_locale_type_mismatch_new() {
        let loc = MessageLocation::new("./messages/zh.json", 8, 1);
        let mismatch = LocaleTypeMismatch::new("zh", ValueType::String, loc);
        assert_eq!(mismatch.locale, "zh");
        assert_eq!(mismatch.actual_type, ValueType::String);
        assert_eq!(mismatch.location.file_path, "./messages/zh.json");
    }
}

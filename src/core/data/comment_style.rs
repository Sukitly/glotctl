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

#[cfg(test)]
mod tests {
    use crate::core::CommentStyle;

    #[test]
    fn test_comment_style_is_jsx() {
        assert!(!CommentStyle::Js.is_jsx());
        assert!(CommentStyle::Jsx.is_jsx());
    }
}

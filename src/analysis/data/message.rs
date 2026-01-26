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

#[cfg(test)]
mod tests {
    use crate::analysis::{MessageContext, MessageLocation};

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
}

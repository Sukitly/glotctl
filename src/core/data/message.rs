use std::{collections::HashMap, fmt};

/// Position information in message/locale files (JSON).
///
/// Represents a specific location in a locale JSON file, used for error reporting
/// when there are issues with translation values.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageLocation {
    /// Path to the locale file (e.g., "./messages/en.json").
    pub file_path: String,
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed).
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

/// Value type for translation messages.
///
/// next-intl supports two value types for translation messages:
///
/// 1. **String**: A simple string value (most common)
///    - JSON: `"key": "value"`
///    - Usage: `t("key")` returns string
///
/// 2. **StringArray**: An array of strings (for rich text)
///    - JSON: `"key": ["Part 1", "Part 2"]`
///    - Usage: `t.raw("key")` returns array
///    - The `raw()` method explicitly requests array type
///
/// Type mismatches are detected when:
/// - Code uses `t.raw("key")` but locale has string value
/// - Code uses `t("key")` expecting string but locale has array
/// - Different locales have different types for the same key
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValueType {
    /// A simple string value: `"submit": "Submit"`
    String,

    /// A string array value: `"rich": ["Bold ", "text"]`
    /// Accessed via `t.raw("rich")` to get the array.
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

/// A single message entry from a locale file.
///
/// Represents one key-value pair from a locale JSON file, with its type
/// and location information for error reporting.
#[derive(Debug, Clone)]
pub struct MessageEntry {
    /// Message context (location, key, value).
    pub context: MessageContext,

    /// Value type (string or string array).
    /// Used to detect type mismatches between code and locale files.
    pub value_type: ValueType,
}

/// All messages for a single locale.
#[derive(Debug, Clone)]
pub struct LocaleMessages {
    /// Locale code (e.g., "en", "zh").
    pub locale: String,
    /// File path of the locale file.
    pub file_path: String,
    /// All message entries, keyed by translation key.
    pub entries: HashMap<String, MessageEntry>,
}

pub type AllLocaleMessages = HashMap<String, LocaleMessages>;

impl LocaleMessages {
    /// Create a new LocaleMessages.
    pub fn new(locale: impl Into<String>, file_path: impl Into<String>) -> Self {
        Self {
            locale: locale.into(),
            file_path: file_path.into(),
            entries: HashMap::new(),
        }
    }

    /// Get a message entry by key.
    pub fn get(&self, key: &str) -> Option<&MessageEntry> {
        self.entries.get(key)
    }

    /// Check if a key exists.
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Get all keys as an iterator.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    /// Get the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_value_type_display() {
        assert_eq!(ValueType::String.to_string(), "string");
        assert_eq!(ValueType::StringArray.to_string(), "array");
    }

    #[test]
    fn test_locale_messages_new() {
        let messages = LocaleMessages::new("en", "./messages/en.json");
        assert!(messages.is_empty());
        assert_eq!(messages.locale, "en");
        assert_eq!(messages.file_path, "./messages/en.json");
    }

    #[test]
    fn test_locale_messages_operations() {
        let mut messages = LocaleMessages::new("en", "./messages/en.json");

        // Add an entry
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Common.submit", "Submit");
        messages.entries.insert(
            "Common.submit".to_string(),
            MessageEntry {
                context: ctx,
                value_type: ValueType::String,
            },
        );

        assert!(!messages.is_empty());
        assert_eq!(messages.len(), 1);
        assert!(messages.contains_key("Common.submit"));
        assert!(!messages.contains_key("Common.cancel"));
        assert!(messages.get("Common.submit").is_some());
        assert!(messages.get("Common.cancel").is_none());
    }

    #[test]
    fn test_locale_messages_keys() {
        let mut messages = LocaleMessages::new("en", "./messages/en.json");

        let loc1 = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx1 = MessageContext::new(loc1, "a", "A");
        messages.entries.insert(
            "a".to_string(),
            MessageEntry {
                context: ctx1,
                value_type: ValueType::String,
            },
        );

        let loc2 = MessageLocation::new("./messages/en.json", 6, 3);
        let ctx2 = MessageContext::new(loc2, "b", "B");
        messages.entries.insert(
            "b".to_string(),
            MessageEntry {
                context: ctx2,
                value_type: ValueType::String,
            },
        );

        let keys: Vec<&String> = messages.keys().collect();
        assert_eq!(keys.len(), 2);
    }
}

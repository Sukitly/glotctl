//! Message types for locale data.
//!
//! These types represent translation messages loaded from JSON locale files.

use std::collections::HashMap;

use crate::analysis::ValueType;

use crate::analysis::MessageContext;

// ============================================================
// Message Types
// ============================================================

/// A single message entry from a locale file.
#[derive(Debug, Clone)]
pub struct MessageEntry {
    /// Message context (location, key, value).
    pub context: MessageContext,
    /// Value type (string or string array).
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

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use crate::analysis::MessageLocation;
    use crate::messages::*;

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

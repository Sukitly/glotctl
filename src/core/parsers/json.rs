use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::core::{
    AllLocaleMessages, LocaleMessages, MessageContext, MessageEntry, MessageLocation, ValueType,
};

/// A warning from scanning message files.
#[derive(Debug, Clone)]
pub struct MessageScanWarning {
    pub file_path: String,
    pub error: String,
}

#[derive(Debug, Default)]
pub struct ScanMessagesResult {
    pub messages: AllLocaleMessages,
    pub warnings: Vec<MessageScanWarning>,
}

pub fn parse_json_file(path: &Path, locale: &str) -> Result<LocaleMessages> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read JSON file: {:?}", path))?;

    let json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON file: {:?}", path))?;

    let file_path = path.to_string_lossy().to_string();
    // Pre-compute line index for O(log n) line lookups
    let line_index = build_line_index(&content);
    let mut messages = LocaleMessages::new(locale.to_string(), file_path.to_string());
    flatten_json(
        &json,
        String::new(),
        &file_path,
        &content,
        &line_index,
        &mut messages,
    );
    Ok(messages)
}

/// Build an index of line start byte offsets for O(log n) line lookups.
///
/// The returned vector contains byte offsets where each line starts.
/// Line 1 starts at offset 0, line 2 starts after the first '\n', etc.
fn build_line_index(content: &str) -> Vec<usize> {
    let mut offsets = vec![0]; // Line 1 starts at offset 0
    for (i, c) in content.char_indices() {
        if c == '\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

/// Find line number for a byte offset using binary search.
///
/// Returns 1-based line number.
fn offset_to_line(line_index: &[usize], offset: usize) -> usize {
    match line_index.binary_search(&offset) {
        Ok(line) => line + 1, // Exact match at line start
        Err(line) => line,    // Falls within this line
    }
}

/// Find the line number where a key appears in the JSON content.
///
/// Searches for each key part in sequence to handle duplicate leaf keys correctly.
/// For example, `Common.submit` finds the `"submit"` that comes after `"Common"`,
/// not a `"submit"` in another namespace like `"Auth"`.
///
/// This function validates that each match is actually a JSON key (followed by `:`)
/// rather than a string value that happens to contain the same text.
///
/// Uses pre-computed line index for O(log n) line number lookup.
fn find_key_line(content: &str, key_path: &str, line_index: &[usize]) -> usize {
    // Split key path: "Common.submit" -> ["Common", "submit"]
    let parts: Vec<&str> = key_path.split('.').collect();

    let mut search_start = 0;
    for part in &parts {
        let pattern = format!("\"{}\"", part);
        let remaining = &content[search_start..];

        // Find all occurrences and pick the one that's a key (followed by :)
        let mut pos = 0;
        let mut found = false;
        while let Some(rel_pos) = remaining[pos..].find(&pattern) {
            let abs_pos = pos + rel_pos;
            let after_pattern = abs_pos + pattern.len();

            // Check if this is a key (followed by optional whitespace and colon)
            if after_pattern < remaining.len() {
                let is_key = remaining[after_pattern..].trim_start().starts_with(':');
                if is_key {
                    search_start += after_pattern;
                    found = true;
                    break;
                }
            }
            pos = abs_pos + 1;
        }

        // If no valid key found for this part, stop searching
        if !found {
            break;
        }
    }

    // Use binary search to find line number (O(log n) instead of O(n))
    if search_start > 0 {
        offset_to_line(line_index, search_start)
    } else {
        1 // Default to line 1 if not found
    }
}

fn flatten_json(
    value: &Value,
    prefix: String,
    file_path: &str,
    content: &str,
    line_index: &[usize],
    result: &mut LocaleMessages,
) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_json(val, new_prefix, file_path, content, line_index, result);
            }
        }
        Value::String(s) => {
            let line = find_key_line(content, &prefix, line_index);
            let context = MessageContext::new(
                MessageLocation::with_line(file_path.to_string(), line),
                prefix.clone(),
                s.clone(),
            );
            result.entries.insert(
                prefix,
                MessageEntry {
                    context,
                    value_type: ValueType::String,
                },
            );
        }
        Value::Array(arr) => {
            // Empty arrays don't produce any keys
            if arr.is_empty() {
                return;
            }

            // Check if this is a string-only array (used with t.raw() or similar)
            // vs an array of objects (accessed with indexed keys like items.0.question)
            let is_string_array = arr.iter().all(|v| matches!(v, Value::String(_)));

            if is_string_array && !prefix.is_empty() {
                // String arrays are accessed as a whole (e.g., t.raw("benefits"))
                // Store as a single key with joined value for display
                let values: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                let line = find_key_line(content, &prefix, line_index);
                let context = MessageContext::new(
                    MessageLocation::with_line(file_path.to_string(), line),
                    prefix.clone(),
                    values.join(", "),
                );
                result.entries.insert(
                    prefix,
                    MessageEntry {
                        context,
                        value_type: ValueType::StringArray,
                    },
                );
            } else {
                // Object arrays or mixed arrays: expand with indices
                // (e.g., faq.items.0.question, faq.items.1.question)
                for (index, val) in arr.iter().enumerate() {
                    let new_prefix = if prefix.is_empty() {
                        index.to_string()
                    } else {
                        format!("{}.{}", prefix, index)
                    };
                    flatten_json(val, new_prefix, file_path, content, line_index, result);
                }
            }
        }
        _ => {}
    }
}

/// Extracts locale from filename.
///
/// Examples:
/// - "en.json" -> Some("en")
/// - "zh-CN.json" -> Some("zh-CN")
/// - "/path/to/messages/ja.json" -> Some("ja")
pub fn extract_locale(path: impl AsRef<Path>) -> Option<String> {
    let path = path.as_ref();
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

pub fn scan_message_files(message_dir: impl AsRef<Path>) -> Result<ScanMessagesResult> {
    let message_dir = message_dir.as_ref();
    let mut result = ScanMessagesResult::default();

    if !message_dir.exists() {
        bail!(
            "Messages directory '{}' does not exist.\n\
             Hint: Check your .glotrc.json 'messagesDir' setting.",
            message_dir.display()
        );
    }

    if !message_dir.is_dir() {
        bail!("'{}' is not a directory.", message_dir.display());
    }

    for entry in fs::read_dir(message_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("json")
            && let Some(locale) = extract_locale(&path)
        {
            match parse_json_file(&path, &locale) {
                Ok(messages) => {
                    result.messages.insert(locale, messages);
                }
                Err(e) => {
                    result.warnings.push(MessageScanWarning {
                        file_path: path.to_string_lossy().to_string(),
                        error: e.to_string(),
                    });
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::core::LocaleMessages;
    use crate::core::parsers::json::*;

    #[test]
    fn test_flatten_simple() {
        let content = r#"{"Common": {"save": "Save", "cancel": "Cancel"}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("Common.save").map(|e| &e.context.value),
            Some(&"Save".to_string())
        );
        assert_eq!(
            result.get("Common.cancel").map(|e| &e.context.value),
            Some(&"Cancel".to_string())
        );
    }

    #[test]
    fn test_flatten_nested() {
        let content = r#"{"Auth": {"Login": {"title": "Login", "button": "Submit"}}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("Auth.Login.title").map(|e| &e.context.value),
            Some(&"Login".to_string())
        );
        assert_eq!(
            result.get("Auth.Login.button").map(|e| &e.context.value),
            Some(&"Submit".to_string())
        );
    }

    #[test]
    fn test_flatten_root_level() {
        let content = r#"{"title": "Hello", "description": "World"}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("title").map(|e| &e.context.value),
            Some(&"Hello".to_string())
        );
        assert_eq!(
            result.get("description").map(|e| &e.context.value),
            Some(&"World".to_string())
        );
    }

    #[test]
    fn test_extract_locale() {
        assert_eq!(extract_locale(Path::new("en.json")), Some("en".to_string()));
        assert_eq!(
            extract_locale(Path::new("zh-CN.json")),
            Some("zh-CN".to_string())
        );
        assert_eq!(
            extract_locale(Path::new("/path/to/messages/ja.json")),
            Some("ja".to_string())
        );
    }

    #[test]
    fn test_find_key_line_skips_value_matches() {
        // Test that find_key_line correctly skips matches in string values
        // and only matches actual keys (followed by :)
        let content = r#"{
  "Auth": {
    "message": "Welcome to Auth page",
    "title": "Auth Login"
  }
}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        // "Auth.title" should point to line 4 (the actual "title" key),
        // not line 3 where "Auth" appears in the value "Welcome to Auth page"
        let entry = result.get("Auth.title").unwrap();
        assert_eq!(
            entry.context.location.line, 4,
            "Expected line 4 for Auth.title key"
        );

        let entry = result.get("Auth.message").unwrap();
        assert_eq!(
            entry.context.location.line, 3,
            "Expected line 3 for Auth.message key"
        );
    }

    #[test]
    fn test_build_line_index() {
        let content = "line1\nline2\nline3";
        let index = build_line_index(content);

        // Line 1 starts at 0, line 2 at 6, line 3 at 12
        assert_eq!(index, vec![0, 6, 12]);

        // Test offset_to_line
        assert_eq!(offset_to_line(&index, 0), 1); // Start of line 1
        assert_eq!(offset_to_line(&index, 3), 1); // Middle of line 1
        assert_eq!(offset_to_line(&index, 6), 2); // Start of line 2
        assert_eq!(offset_to_line(&index, 8), 2); // Middle of line 2
        assert_eq!(offset_to_line(&index, 12), 3); // Start of line 3
    }

    #[test]
    fn test_parse_json_file() {
        use std::io::Write;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("en.json");

        let mut file = fs::File::create(&file_path).unwrap();
        write!(file, r#"{{"Common": {{"submit": "Submit"}}}}"#).unwrap();

        let messages = parse_json_file(&file_path, "en").unwrap();
        let entry = messages.get("Common.submit").unwrap();
        assert_eq!(entry.context.value, "Submit");
        assert!(entry.context.location.file_path.ends_with("en.json"));
    }

    #[test]
    fn test_scan_message_files() {
        use std::io::Write;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let messages_dir = dir.path();

        // Create valid JSON files
        let mut en_file = fs::File::create(messages_dir.join("en.json")).unwrap();
        write!(en_file, r#"{{"submit": "Submit"}}"#).unwrap();

        let mut zh_file = fs::File::create(messages_dir.join("zh.json")).unwrap();
        write!(zh_file, r#"{{"submit": "提交"}}"#).unwrap();

        let result = scan_message_files(messages_dir).unwrap();

        assert_eq!(result.messages.len(), 2);
        assert!(result.messages.contains_key("en"));
        assert!(result.messages.contains_key("zh"));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_scan_message_files_with_invalid_json() {
        use std::io::Write;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let messages_dir = dir.path();

        // Create one valid and one invalid JSON file
        let mut en_file = fs::File::create(messages_dir.join("en.json")).unwrap();
        write!(en_file, r#"{{"submit": "Submit"}}"#).unwrap();

        let mut zh_file = fs::File::create(messages_dir.join("zh.json")).unwrap();
        write!(zh_file, r#"{{ invalid json }}"#).unwrap();

        let result = scan_message_files(messages_dir).unwrap();

        // Valid file should be parsed
        assert_eq!(result.messages.len(), 1);
        assert!(result.messages.contains_key("en"));

        // Invalid file should produce a warning
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].file_path.contains("zh.json"));
    }

    #[test]
    fn test_scan_message_files_nonexistent_dir() {
        let result = scan_message_files(Path::new("/nonexistent/path"));

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"));
        assert!(err.contains("messagesDir"));
    }

    #[test]
    fn test_flatten_array() {
        let content = r#"{"faq": {"items": [{"question": "Q1", "answer": "A1"}, {"question": "Q2", "answer": "A2"}]}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("faq.items.0.question").map(|e| &e.context.value),
            Some(&"Q1".to_string())
        );
        assert_eq!(
            result.get("faq.items.0.answer").map(|e| &e.context.value),
            Some(&"A1".to_string())
        );
        assert_eq!(
            result.get("faq.items.1.question").map(|e| &e.context.value),
            Some(&"Q2".to_string())
        );
        assert_eq!(
            result.get("faq.items.1.answer").map(|e| &e.context.value),
            Some(&"A2".to_string())
        );
    }

    #[test]
    fn test_flatten_root_array() {
        let content = r#"["first", "second", "third"]"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("0").map(|e| &e.context.value),
            Some(&"first".to_string())
        );
        assert_eq!(
            result.get("1").map(|e| &e.context.value),
            Some(&"second".to_string())
        );
        assert_eq!(
            result.get("2").map(|e| &e.context.value),
            Some(&"third".to_string())
        );
    }

    #[test]
    fn test_flatten_nested_array() {
        let content = r#"{"Page": {"steps": [{"title": "Step 1"}, {"title": "Step 2"}]}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("Page.steps.0.title").map(|e| &e.context.value),
            Some(&"Step 1".to_string())
        );
        assert_eq!(
            result.get("Page.steps.1.title").map(|e| &e.context.value),
            Some(&"Step 2".to_string())
        );
    }

    #[test]
    fn test_flatten_string_array_not_expanded() {
        // String arrays should be treated as a single key (accessed via t.raw())
        // NOT expanded into individual indexed keys
        let content = r#"{"Page": {"benefits": ["Fast", "Easy", "Reliable"]}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        // String array should be a single key, not expanded
        assert!(
            result.contains_key("Page.benefits"),
            "String array should be stored as single key"
        );
        assert_eq!(
            result.get("Page.benefits").map(|e| &e.context.value),
            Some(&"Fast, Easy, Reliable".to_string())
        );

        // Should NOT have indexed keys
        assert!(
            !result.contains_key("Page.benefits.0"),
            "String array should NOT be expanded to indexed keys"
        );
        assert!(
            !result.contains_key("Page.benefits.1"),
            "String array should NOT be expanded to indexed keys"
        );
    }

    #[test]
    fn test_flatten_mixed_array_expanded() {
        // Arrays with non-string elements should be expanded
        let content = r#"{"Page": {"items": [{"name": "A"}, {"name": "B"}]}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        // Object array should be expanded
        assert!(result.contains_key("Page.items.0.name"));
        assert!(result.contains_key("Page.items.1.name"));
        // Should NOT have the array as a single key
        assert!(!result.contains_key("Page.items"));
    }

    #[test]
    fn test_flatten_empty_array() {
        // Empty arrays should not produce any keys
        let content = r#"{"Page": {"items": []}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        // Empty array should not create any keys
        assert!(result.is_empty(), "Empty array should not produce any keys");
    }

    #[test]
    fn test_flatten_single_element_string_array() {
        // Single element string array should still be treated as string array
        let content = r#"{"Page": {"tags": ["only-tag"]}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        // Should be stored as single key
        assert!(result.contains_key("Page.tags"));
        assert_eq!(
            result.get("Page.tags").map(|e| &e.context.value),
            Some(&"only-tag".to_string())
        );
        // Should NOT be expanded
        assert!(!result.contains_key("Page.tags.0"));
    }

    #[test]
    fn test_flatten_mixed_type_array_expanded() {
        // Arrays with mixed types (not all strings) should be expanded
        let content = r#"{"Page": {"data": ["text", 123, true]}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        // Mixed array should be expanded, but only string elements create entries
        // (numbers and booleans are ignored by flatten_json)
        assert!(
            result.contains_key("Page.data.0"),
            "String element should be expanded"
        );
        assert_eq!(
            result.get("Page.data.0").map(|e| &e.context.value),
            Some(&"text".to_string())
        );
        // Non-string elements don't create keys
        assert!(!result.contains_key("Page.data.1"));
        assert!(!result.contains_key("Page.data.2"));
    }

    #[test]
    fn test_flatten_empty_string_in_array() {
        // Array with empty strings should still be treated as string array
        let content = r#"{"Page": {"values": ["", "valid", ""]}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = LocaleMessages::new("en", "test.json");
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        // Should be stored as single key (all elements are strings)
        assert!(result.contains_key("Page.values"));
        assert_eq!(
            result.get("Page.values").map(|e| &e.context.value),
            Some(&", valid, ".to_string())
        );
    }
}

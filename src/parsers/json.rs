use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub value: String,
    pub file_path: String,
    pub line: usize,
}

pub type MessageMap = HashMap<String, MessageEntry>;

#[derive(Debug, Default)]
pub struct ScanMessagesResult {
    pub messages: HashMap<String, MessageMap>,
    pub warnings: Vec<String>,
}

pub fn parse_json_file(path: &Path) -> Result<MessageMap> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read JSON file: {:?}", path))?;

    let json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON file: {:?}", path))?;

    let file_path = path.to_string_lossy().to_string();
    // Pre-compute line index for O(log n) line lookups
    let line_index = build_line_index(&content);
    let mut message_map = MessageMap::new();
    flatten_json(
        &json,
        String::new(),
        &file_path,
        &content,
        &line_index,
        &mut message_map,
    );
    Ok(message_map)
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
    result: &mut MessageMap,
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
            result.insert(
                prefix,
                MessageEntry {
                    value: s.clone(),
                    file_path: file_path.to_string(),
                    line,
                },
            );
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
            match parse_json_file(&path) {
                Ok(messages) => {
                    result.messages.insert(locale, messages);
                }
                Err(e) => {
                    result
                        .warnings
                        .push(format!("Failed to parse {:?}: {}", path, e));
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flatten_simple() {
        let content = r#"{"Common": {"save": "Save", "cancel": "Cancel"}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = MessageMap::new();
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("Common.save").map(|e| &e.value),
            Some(&"Save".to_string())
        );
        assert_eq!(
            result.get("Common.cancel").map(|e| &e.value),
            Some(&"Cancel".to_string())
        );
    }

    #[test]
    fn test_flatten_nested() {
        let content = r#"{"Auth": {"Login": {"title": "Login", "button": "Submit"}}}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = MessageMap::new();
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("Auth.Login.title").map(|e| &e.value),
            Some(&"Login".to_string())
        );
        assert_eq!(
            result.get("Auth.Login.button").map(|e| &e.value),
            Some(&"Submit".to_string())
        );
    }

    #[test]
    fn test_flatten_root_level() {
        let content = r#"{"title": "Hello", "description": "World"}"#;
        let json: Value = serde_json::from_str(content).unwrap();
        let line_index = build_line_index(content);

        let mut result = MessageMap::new();
        flatten_json(
            &json,
            String::new(),
            "test.json",
            content,
            &line_index,
            &mut result,
        );

        assert_eq!(
            result.get("title").map(|e| &e.value),
            Some(&"Hello".to_string())
        );
        assert_eq!(
            result.get("description").map(|e| &e.value),
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

        let mut result = MessageMap::new();
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
        assert_eq!(entry.line, 4, "Expected line 4 for Auth.title key");

        let entry = result.get("Auth.message").unwrap();
        assert_eq!(entry.line, 3, "Expected line 3 for Auth.message key");
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

        let messages = parse_json_file(&file_path).unwrap();
        let entry = messages.get("Common.submit").unwrap();
        assert_eq!(entry.value, "Submit");
        assert!(entry.file_path.ends_with("en.json"));
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
        assert!(result.warnings[0].contains("zh.json"));
    }

    #[test]
    fn test_scan_message_files_nonexistent_dir() {
        let result = scan_message_files(Path::new("/nonexistent/path"));

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"));
        assert!(err.contains("messagesDir"));
    }
}

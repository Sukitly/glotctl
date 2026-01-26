use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::Value;

/// A JSON editor that deletes keys by their key paths.
///
/// Uses serde_json for parsing and serialization.
/// Output is always pretty-printed with 2-space indentation.
pub struct JsonEditor {
    file_path: PathBuf,
    content: String,
}

impl JsonEditor {
    /// Open a JSON file for editing.
    pub fn open(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        Ok(Self {
            file_path: path.to_path_buf(),
            content,
        })
    }

    /// Delete keys by their full key paths (e.g., "Common.submit").
    ///
    /// This method parses the JSON, removes the specified keys,
    /// removes empty parent objects, and re-serializes with 2-space indentation.
    pub fn delete_keys(&mut self, key_paths: &[&str]) -> Result<()> {
        // Parse the content as JSON
        let mut value: Value =
            serde_json::from_str(&self.content).with_context(|| "Failed to parse JSON")?;

        // Delete each key path
        for key_path in key_paths {
            delete_key_path(&mut value, key_path);
        }

        // Remove empty objects
        remove_empty_objects(&mut value);

        // Re-serialize with 2-space indentation
        self.content =
            serde_json::to_string_pretty(&value).with_context(|| "Failed to serialize JSON")?;

        Ok(())
    }

    /// Save the modified content back to the file.
    pub fn save(&self) -> Result<()> {
        // Ensure trailing newline
        let content = if self.content.ends_with('\n') {
            self.content.clone()
        } else {
            format!("{}\n", self.content)
        };

        fs::write(&self.file_path, content)
            .with_context(|| format!("Failed to write file: {}", self.file_path.display()))?;

        Ok(())
    }

    /// Get the current content as a string (for testing).
    #[cfg(test)]
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// Delete a key path from a JSON value (e.g., "Common.submit").
fn delete_key_path(value: &mut Value, key_path: &str) {
    let parts: Vec<&str> = key_path.split('.').collect();
    if parts.is_empty() {
        return;
    }

    // Navigate to the parent object
    let mut current = value;
    for part in &parts[..parts.len() - 1] {
        match current {
            Value::Object(map) => {
                if let Some(child) = map.get_mut(*part) {
                    current = child;
                } else {
                    return; // Key path doesn't exist
                }
            }
            _ => return, // Not an object
        }
    }

    // Delete the final key using shift_remove to preserve order
    if let Value::Object(map) = current {
        let final_key = parts[parts.len() - 1];
        map.shift_remove(final_key);
    }
}

/// Recursively remove empty objects from a JSON value.
fn remove_empty_objects(value: &mut Value) {
    if let Value::Object(map) = value {
        // First, recursively clean children
        for child in map.values_mut() {
            remove_empty_objects(child);
        }

        // Then, remove any children that are empty objects
        map.retain(|_, v| !matches!(v, Value::Object(m) if m.is_empty()));
    }
}

#[cfg(test)]
mod tests {
    use crate::json_editor::*;
    use tempfile::TempDir;

    fn create_temp_json(content: &str) -> (TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.json");
        fs::write(&file_path, content).unwrap();
        (temp_dir, file_path)
    }

    #[test]
    fn test_delete_simple_key() {
        let json = r#"{"key1": "value1", "key2": "value2", "key3": "value3"}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&["key2"]).unwrap();

        let expected = r#"{
  "key1": "value1",
  "key3": "value3"
}"#;
        assert_eq!(editor.content(), expected);
    }

    #[test]
    fn test_delete_nested_key() {
        let json = r#"{"Common": {"submit": "Submit", "cancel": "Cancel"}, "Other": "value"}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&["Common.submit"]).unwrap();

        let expected = r#"{
  "Common": {
    "cancel": "Cancel"
  },
  "Other": "value"
}"#;
        assert_eq!(editor.content(), expected);
    }

    #[test]
    fn test_delete_removes_empty_parent() {
        let json = r#"{"Common": {"submit": "Submit"}, "Other": "value"}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&["Common.submit"]).unwrap();

        // Common should be removed since it's now empty
        let expected = r#"{
  "Other": "value"
}"#;
        assert_eq!(editor.content(), expected);
    }

    #[test]
    fn test_delete_multiple_keys() {
        let json = r#"{"key1": "v1", "key2": "v2", "key3": "v3", "key4": "v4"}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&["key2", "key4"]).unwrap();

        let expected = r#"{
  "key1": "v1",
  "key3": "v3"
}"#;
        assert_eq!(editor.content(), expected);
    }

    #[test]
    fn test_preserves_key_order() {
        let json = r#"{"z": "last", "a": "first", "m": "middle"}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&["a"]).unwrap();

        // Key order should be preserved (z, m)
        let content = editor.content();
        let z_pos = content.find("\"z\"").unwrap();
        let m_pos = content.find("\"m\"").unwrap();
        assert!(z_pos < m_pos, "Key order should be preserved");
    }

    #[test]
    fn test_handles_deeply_nested() {
        let json = r#"{"a": {"b": {"c": {"d": "deep"}}}}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&["a.b.c.d"]).unwrap();

        // All empty parents should be removed
        let expected = "{}";
        assert_eq!(editor.content(), expected);
    }

    #[test]
    fn test_delete_nonexistent_key() {
        let json = r#"{"key1": "value1"}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        // Deleting non-existent key should not error
        editor.delete_keys(&["nonexistent"]).unwrap();

        // Original content should be unchanged (just reformatted)
        let expected = r#"{
  "key1": "value1"
}"#;
        assert_eq!(editor.content(), expected);
    }

    #[test]
    fn test_delete_nonexistent_nested_key() {
        let json = r#"{"Common": {"submit": "Submit"}}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        // Deleting non-existent nested key should not error
        editor.delete_keys(&["Common.nonexistent"]).unwrap();
        editor.delete_keys(&["Nonexistent.key"]).unwrap();

        let expected = r#"{
  "Common": {
    "submit": "Submit"
  }
}"#;
        assert_eq!(editor.content(), expected);
    }

    #[test]
    fn test_delete_from_empty_object() {
        let json = r#"{}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        // Deleting from empty object should not error
        editor.delete_keys(&["key"]).unwrap();

        assert_eq!(editor.content(), "{}");
    }

    #[test]
    fn test_save_adds_trailing_newline() {
        let json = r#"{"key": "value"}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&[]).unwrap();
        editor.save().unwrap();

        // Read the file directly and verify trailing newline
        let saved_content = fs::read_to_string(&path).unwrap();
        assert!(
            saved_content.ends_with('\n'),
            "File should end with newline"
        );
    }

    #[test]
    fn test_delete_all_keys() {
        let json = r#"{"key1": "v1", "key2": "v2"}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&["key1", "key2"]).unwrap();

        assert_eq!(editor.content(), "{}");
    }

    #[test]
    fn test_delete_with_unicode_values() {
        let json = r#"{"Common": {"hello": "你好", "world": "世界"}}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        editor.delete_keys(&["Common.hello"]).unwrap();

        let content = editor.content();
        assert!(!content.contains("你好"));
        assert!(content.contains("世界"));
    }

    #[test]
    fn test_delete_partial_nested_keeps_siblings() {
        let json = r#"{"Common": {"a": "1", "b": "2"}, "Other": {"x": "9"}}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        // Delete one key from Common
        editor.delete_keys(&["Common.a"]).unwrap();

        let content = editor.content();
        assert!(!content.contains("\"a\""));
        assert!(content.contains("\"b\""));
        assert!(content.contains("\"Other\""));
        assert!(content.contains("\"x\""));
    }

    #[test]
    fn test_delete_preserves_order_after_multiple_deletions() {
        // Same scenario as the CLI test
        let json = r#"{"Common": {"zebra": "Zebra", "alpha": "Alpha", "middle": "Middle", "gamma": "Gamma"}}"#;
        let (_temp, path) = create_temp_json(json);
        let mut editor = JsonEditor::open(&path).unwrap();

        // Delete zebra and middle, keeping alpha and gamma
        editor
            .delete_keys(&["Common.zebra", "Common.middle"])
            .unwrap();

        let content = editor.content();

        // alpha should come before gamma (original order preserved)
        let alpha_pos = content.find("\"alpha\"").unwrap();
        let gamma_pos = content.find("\"gamma\"").unwrap();
        assert!(
            alpha_pos < gamma_pos,
            "Key order should be preserved: alpha at {} should be before gamma at {}",
            alpha_pos,
            gamma_pos
        );
    }
}

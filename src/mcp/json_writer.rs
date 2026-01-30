//! JSON writer for MCP translations.
//!
//! Supports nested dot-keys and preserves key order.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Added,
    Updated,
}

impl KeyAction {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyAction::Added => "added",
            KeyAction::Updated => "updated",
        }
    }
}

#[derive(Debug)]
pub struct JsonWriter {
    file_path: PathBuf,
    root: Value,
}

impl JsonWriter {
    pub fn open_or_create(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let root = if path.exists() {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            if content.trim().is_empty() {
                Value::Object(Map::new())
            } else {
                serde_json::from_str(&content)
                    .with_context(|| format!("Failed to parse JSON: {}", path.display()))?
            }
        } else {
            Value::Object(Map::new())
        };

        Ok(Self {
            file_path: path.to_path_buf(),
            root,
        })
    }

    pub fn add_value(&mut self, key: &str, value: Value) -> KeyAction {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.is_empty() {
            return KeyAction::Updated;
        }

        let mut current = &mut self.root;
        for part in &parts[..parts.len().saturating_sub(1)] {
            if !current.is_object() {
                *current = Value::Object(Map::new());
            }

            let obj = current.as_object_mut().unwrap();
            current = obj
                .entry(part.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
        }

        if !current.is_object() {
            *current = Value::Object(Map::new());
        }

        let obj = current.as_object_mut().unwrap();
        let final_key = parts[parts.len() - 1];
        let action = if obj.contains_key(final_key) {
            KeyAction::Updated
        } else {
            KeyAction::Added
        };
        obj.insert(final_key.to_string(), value);
        action
    }

    pub fn save(&self) -> Result<()> {
        let mut content =
            serde_json::to_string_pretty(&self.root).context("Failed to serialize JSON")?;
        if !content.ends_with('\n') {
            content.push('\n');
        }

        fs::write(&self.file_path, content)
            .with_context(|| format!("Failed to write file: {}", self.file_path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn temp_json_path(temp_dir: &TempDir, name: &str) -> PathBuf {
        temp_dir.path().join(name)
    }

    #[test]
    fn test_open_or_create_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "new.json");

        let writer = JsonWriter::open_or_create(&path).unwrap();

        assert_eq!(writer.root, json!({}));
        assert_eq!(writer.file_path, path);
    }

    #[test]
    fn test_open_or_create_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "existing.json");

        fs::write(&path, r#"{"key": "value", "num": 42}"#).unwrap();

        let writer = JsonWriter::open_or_create(&path).unwrap();

        assert_eq!(writer.root, json!({"key": "value", "num": 42}));
    }

    #[test]
    fn test_open_or_create_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "empty.json");

        fs::write(&path, "").unwrap();

        let writer = JsonWriter::open_or_create(&path).unwrap();

        assert_eq!(writer.root, json!({}));
    }

    #[test]
    fn test_open_or_create_whitespace_only_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "whitespace.json");

        fs::write(&path, "  \n\t  \n").unwrap();

        let writer = JsonWriter::open_or_create(&path).unwrap();

        assert_eq!(writer.root, json!({}));
    }

    #[test]
    fn test_open_or_create_malformed_json() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "malformed.json");

        fs::write(&path, "{invalid json}").unwrap();

        let result = JsonWriter::open_or_create(&path);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse JSON")
        );
    }

    #[test]
    fn test_add_value_simple_key() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        let action = writer.add_value("key", json!("value"));

        assert_eq!(action, KeyAction::Added);
        assert_eq!(writer.root, json!({"key": "value"}));
    }

    #[test]
    fn test_add_value_nested_key() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        let action = writer.add_value("common.submit", json!("Submit"));

        assert_eq!(action, KeyAction::Added);
        assert_eq!(writer.root, json!({"common": {"submit": "Submit"}}));
    }

    #[test]
    fn test_add_value_deep_nesting() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        let action = writer.add_value("a.b.c.d.e", json!("deep"));

        assert_eq!(action, KeyAction::Added);
        assert_eq!(
            writer.root,
            json!({"a": {"b": {"c": {"d": {"e": "deep"}}}}})
        );
    }

    #[test]
    fn test_add_value_creates_intermediate_objects() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        writer.add_value("existing.key", json!("first"));
        let action = writer.add_value("existing.other.nested", json!("second"));

        assert_eq!(action, KeyAction::Added);
        assert_eq!(
            writer.root,
            json!({"existing": {"key": "first", "other": {"nested": "second"}}})
        );
    }

    #[test]
    fn test_add_value_overwrites_non_object_intermediate() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        fs::write(&path, r#"{"items": "wrong type"}"#).unwrap();

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        let action = writer.add_value("items.nested.key", json!("value"));

        assert_eq!(action, KeyAction::Added);
        assert_eq!(writer.root, json!({"items": {"nested": {"key": "value"}}}));
    }

    #[test]
    fn test_add_value_returns_added_for_new_key() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        let action = writer.add_value("newKey", json!("value"));

        assert_eq!(action, KeyAction::Added);
    }

    #[test]
    fn test_add_value_returns_updated_for_existing_key() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        fs::write(&path, r#"{"key": "old value"}"#).unwrap();

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        let action = writer.add_value("key", json!("new value"));

        assert_eq!(action, KeyAction::Updated);
        assert_eq!(writer.root, json!({"key": "new value"}));
    }

    #[test]
    fn test_add_value_supports_array_values() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        writer.add_value("items", json!(["one", "two", "three"]));

        assert_eq!(writer.root, json!({"items": ["one", "two", "three"]}));
    }

    #[test]
    fn test_add_value_supports_object_values() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "test.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        writer.add_value("config", json!({"setting": "value", "enabled": true}));

        assert_eq!(
            writer.root,
            json!({"config": {"setting": "value", "enabled": true}})
        );
    }

    #[test]
    fn test_save_writes_formatted_json() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "output.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        writer.add_value("key1", json!("value1"));
        writer.add_value("key2", json!(42));
        writer.save().unwrap();

        let content = fs::read_to_string(&path).unwrap();

        // Check pretty formatting (multiline)
        assert!(content.contains('\n'));
        // Verify content
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed, json!({"key1": "value1", "key2": 42}));
    }

    #[test]
    fn test_save_adds_trailing_newline() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "output.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();
        writer.add_value("test", json!("value"));
        writer.save().unwrap();

        let content = fs::read_to_string(&path).unwrap();

        assert!(content.ends_with('\n'));
    }

    #[test]
    fn test_key_action_as_str() {
        assert_eq!(KeyAction::Added.as_str(), "added");
        assert_eq!(KeyAction::Updated.as_str(), "updated");
    }

    #[test]
    fn test_multiple_operations() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_json_path(&temp_dir, "multi.json");

        let mut writer = JsonWriter::open_or_create(&path).unwrap();

        assert_eq!(writer.add_value("a", json!(1)), KeyAction::Added);
        assert_eq!(writer.add_value("b.c", json!(2)), KeyAction::Added);
        assert_eq!(writer.add_value("a", json!(3)), KeyAction::Updated);
        assert_eq!(writer.add_value("b.d", json!(4)), KeyAction::Added);

        assert_eq!(writer.root, json!({"a": 3, "b": {"c": 2, "d": 4}}));
    }
}

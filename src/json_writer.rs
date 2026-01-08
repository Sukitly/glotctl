use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

/// Action taken on a key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Added,
    Updated,
}

impl KeyAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyAction::Added => "added",
            KeyAction::Updated => "updated",
        }
    }
}

/// JSON writer for locale files with nested structure support
pub struct JsonWriter {
    file_path: PathBuf,
    data: Map<String, Value>,
}

impl JsonWriter {
    /// Open an existing JSON file or create a new empty one
    pub fn open_or_create(path: &Path) -> Result<Self> {
        let data = if path.exists() {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            let value: Value = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse JSON: {}", path.display()))?;
            match value {
                Value::Object(map) => map,
                _ => bail!("Root of JSON file must be an object: {}", path.display()),
            }
        } else {
            Map::new()
        };

        Ok(Self {
            file_path: path.to_path_buf(),
            data,
        })
    }

    /// Add a key-value pair at a nested path
    ///
    /// Key format: "Navigation.signOut" or "HomePage.cta.startNow"
    /// Creates intermediate objects if they don't exist.
    /// Accepts any serde_json::Value (string, array, etc.)
    pub fn add_value(&mut self, key: &str, value: Value) -> KeyAction {
        let parts: Vec<&str> = key.split('.').collect();
        insert_nested(&mut self.data, &parts, value)
    }

    /// Save the JSON file with pretty formatting
    ///
    /// Uses 2-space indentation and adds a trailing newline.
    pub fn save(&self) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(&Value::Object(self.data.clone()))
            .context("Failed to serialize JSON")?;

        fs::write(&self.file_path, format!("{}\n", content))
            .with_context(|| format!("Failed to write file: {}", self.file_path.display()))?;

        Ok(())
    }
}

/// Insert a value at a nested path, creating intermediate objects as needed
fn insert_nested(root: &mut Map<String, Value>, path: &[&str], value: Value) -> KeyAction {
    if path.is_empty() {
        return KeyAction::Added; // Edge case: empty path
    }

    if path.len() == 1 {
        // Final key - insert the value
        let key = path[0].to_string();
        let action = if root.contains_key(&key) {
            KeyAction::Updated
        } else {
            KeyAction::Added
        };
        root.insert(key, value);
        return action;
    }

    // Navigate/create intermediate objects
    let key = path[0].to_string();
    let next_level = root.entry(key).or_insert_with(|| Value::Object(Map::new()));

    // If the existing value is not an object, replace it with an object
    if !next_level.is_object() {
        *next_level = Value::Object(Map::new());
    }

    let inner_map = next_level.as_object_mut().unwrap();
    insert_nested(inner_map, &path[1..], value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_add_simple_key() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");

        let mut writer = JsonWriter::open_or_create(&file_path).unwrap();
        let action = writer.add_value("hello", json!("world"));
        writer.save().unwrap();

        assert_eq!(action, KeyAction::Added);

        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["hello"], "world");
    }

    #[test]
    fn test_add_nested_key() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");

        let mut writer = JsonWriter::open_or_create(&file_path).unwrap();
        writer.add_value("Navigation.signOut", json!("Sign Out"));
        writer.add_value("HomePage.cta.startNow", json!("Start Now"));
        writer.save().unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed["Navigation"]["signOut"], "Sign Out");
        assert_eq!(parsed["HomePage"]["cta"]["startNow"], "Start Now");
    }

    #[test]
    fn test_update_existing_key() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");

        // Create initial file
        fs::write(&file_path, r#"{"Navigation": {"signOut": "Old Value"}}"#).unwrap();

        let mut writer = JsonWriter::open_or_create(&file_path).unwrap();
        let action = writer.add_value("Navigation.signOut", json!("New Value"));
        writer.save().unwrap();

        assert_eq!(action, KeyAction::Updated);

        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["Navigation"]["signOut"], "New Value");
    }

    #[test]
    fn test_preserve_existing_keys() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");

        // Create initial file with existing content
        fs::write(
            &file_path,
            r#"{
  "Navigation": {
    "home": "Home",
    "about": "About"
  }
}"#,
        )
        .unwrap();

        let mut writer = JsonWriter::open_or_create(&file_path).unwrap();
        writer.add_value("Navigation.signOut", json!("Sign Out"));
        writer.save().unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Existing keys should be preserved
        assert_eq!(parsed["Navigation"]["home"], "Home");
        assert_eq!(parsed["Navigation"]["about"], "About");
        // New key should be added
        assert_eq!(parsed["Navigation"]["signOut"], "Sign Out");
    }

    #[test]
    fn test_create_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("subdir").join("test.json");

        let mut writer = JsonWriter::open_or_create(&file_path).unwrap();
        writer.add_value("hello", json!("world"));
        writer.save().unwrap();

        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["hello"], "world");
    }

    #[test]
    fn test_add_value_string_array() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");

        let mut writer = JsonWriter::open_or_create(&file_path).unwrap();
        let array = Value::Array(vec![
            Value::String("item1".to_string()),
            Value::String("item2".to_string()),
        ]);
        let action = writer.add_value("Common.items", array);
        writer.save().unwrap();

        assert_eq!(action, KeyAction::Added);

        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["Common"]["items"][0], "item1");
        assert_eq!(parsed["Common"]["items"][1], "item2");
    }

    #[test]
    fn test_add_value_update_existing() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");

        // Create initial file with a string value
        fs::write(&file_path, r#"{"Common": {"items": "old value"}}"#).unwrap();

        let mut writer = JsonWriter::open_or_create(&file_path).unwrap();
        let array = Value::Array(vec![
            Value::String("new1".to_string()),
            Value::String("new2".to_string()),
        ]);
        let action = writer.add_value("Common.items", array);
        writer.save().unwrap();

        assert_eq!(action, KeyAction::Updated);

        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["Common"]["items"][0], "new1");
        assert_eq!(parsed["Common"]["items"][1], "new2");
    }
}

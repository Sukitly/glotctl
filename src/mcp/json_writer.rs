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

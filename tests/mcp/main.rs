use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use tempfile::TempDir;

mod tools;

/// Test fixture for MCP integration tests
///
/// Manages a temporary Next.js-like project structure with messages/ directory
pub struct McpTestFixture {
    _temp_dir: TempDir,
    project_root: PathBuf,
}

impl McpTestFixture {
    /// Create an empty test project
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let project_root = temp_dir.path().canonicalize()?;

        // Create messages directory
        let messages_dir = project_root.join("messages");
        fs::create_dir_all(&messages_dir)?;

        Ok(Self {
            _temp_dir: temp_dir,
            project_root,
        })
    }

    /// Create a test project with locale message files
    ///
    /// # Example
    /// ```ignore
    /// let fixture = McpTestFixture::with_messages(vec![
    ///     ("en", json!({"key": "value"})),
    ///     ("zh", json!({"key": "值"})),
    /// ])?;
    /// ```
    pub fn with_messages(locales: Vec<(&str, Value)>) -> Result<Self> {
        let fixture = Self::new()?;
        for (locale, content) in locales {
            fixture.write_locale_file(locale, &content)?;
        }
        Ok(fixture)
    }

    /// Write a locale JSON file to messages/<locale>.json
    pub fn write_locale_file(&self, locale: &str, content: &Value) -> Result<()> {
        let path = self
            .project_root
            .join("messages")
            .join(format!("{}.json", locale));
        let json_str = serde_json::to_string_pretty(content)
            .with_context(|| format!("Failed to serialize JSON for locale: {}", locale))?;
        fs::write(&path, format!("{}\n", json_str))
            .with_context(|| format!("Failed to write locale file: {}", path.display()))?;
        Ok(())
    }

    /// Read a locale JSON file from messages/<locale>.json
    pub fn read_locale_file(&self, locale: &str) -> Result<Value> {
        let path = self
            .project_root
            .join("messages")
            .join(format!("{}.json", locale));
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read locale file: {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON from: {}", path.display()))
    }

    /// Write a TSX/JSX file for hardcoded text testing
    pub fn write_tsx_file(&self, relative_path: &str, content: &str) -> Result<()> {
        let path = self.project_root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, content)
            .with_context(|| format!("Failed to write TSX file: {}", path.display()))?;
        Ok(())
    }

    /// Write a .glotrc.json config file
    pub fn write_config(&self, content: &Value) -> Result<()> {
        let path = self.project_root.join(".glotrc.json");
        let json_str = serde_json::to_string_pretty(content)?;
        fs::write(&path, format!("{}\n", json_str))?;
        Ok(())
    }

    /// Get the project root path as a string (for MCP parameters)
    pub fn root(&self) -> String {
        self.project_root.to_string_lossy().to_string()
    }

    /// Get the project root path as a Path reference
    pub fn root_path(&self) -> &Path {
        &self.project_root
    }
}

// ============================================================================
// Fixture Generators
// ============================================================================

/// Create a fixture with type mismatches between locales
pub fn fixture_with_type_mismatch() -> Result<McpTestFixture> {
    McpTestFixture::with_messages(vec![
        (
            "en",
            json!({
                "items": ["one", "two"],
                "nested": {"key": "value"}
            }),
        ),
        (
            "zh",
            json!({
                "items": "wrong type",  // Should be array
                "nested": "also wrong"   // Should be object
            }),
        ),
    ])
}

/// Create a fixture with multiple locales and various data
pub fn fixture_multi_locale() -> Result<McpTestFixture> {
    McpTestFixture::with_messages(vec![
        (
            "en",
            json!({
                "common": {
                    "submit": "Submit",
                    "cancel": "Cancel"
                },
                "home": {
                    "title": "Welcome"
                }
            }),
        ),
        (
            "zh",
            json!({
                "common": {
                    "submit": "提交",
                    "cancel": "取消"
                },
                "home": {
                    "title": "欢迎"
                }
            }),
        ),
        (
            "ja",
            json!({
                "common": {
                    "submit": "送信",
                    "cancel": "キャンセル"
                },
                "home": {
                    "title": "ようこそ"
                }
            }),
        ),
    ])
}

/// Create a fixture with missing keys (replica lag scenario)
pub fn fixture_with_replica_lag() -> Result<McpTestFixture> {
    McpTestFixture::with_messages(vec![
        (
            "en",
            json!({
                "common": {
                    "submit": "Submit",
                    "cancel": "Cancel",
                    "new_key": "New Feature"
                }
            }),
        ),
        (
            "zh",
            json!({
                "common": {
                    "submit": "提交",
                    "cancel": "取消"
                    // Missing "new_key"
                }
            }),
        ),
    ])
}

/// Create a fixture with untranslated values (same as primary locale)
pub fn fixture_with_untranslated() -> Result<McpTestFixture> {
    McpTestFixture::with_messages(vec![
        (
            "en",
            json!({
                "common": {
                    "submit": "Submit",
                    "greeting": "Hello"
                }
            }),
        ),
        (
            "zh",
            json!({
                "common": {
                    "submit": "Submit",  // Not translated, same as English
                    "greeting": "你好"
                }
            }),
        ),
    ])
}

// ============================================================================
// Assertion Helpers
// ============================================================================

/// Assert pagination fields in a scan result
pub fn assert_pagination(
    result: &Value,
    expected_offset: usize,
    expected_limit: usize,
    expected_has_more: bool,
) {
    let pagination = &result["pagination"];
    assert_eq!(
        pagination["offset"].as_u64().unwrap(),
        expected_offset as u64,
        "Pagination offset mismatch"
    );
    assert_eq!(
        pagination["limit"].as_u64().unwrap(),
        expected_limit as u64,
        "Pagination limit mismatch"
    );
    assert_eq!(
        pagination["hasMore"].as_bool().unwrap(),
        expected_has_more,
        "Pagination hasMore mismatch"
    );
}

/// Extract JSON value from a successful CallToolResult
///
/// Panics if the result indicates an error or cannot be parsed
pub fn extract_tool_result_json(result: &rmcp::model::CallToolResult) -> Value {
    // Check for errors using is_error field
    if let Some(true) = result.is_error {
        panic!("Tool call returned an error: {:?}", result);
    }

    assert!(
        !result.content.is_empty(),
        "Tool result should have content"
    );

    // Extract text from the content
    let content_item = &result.content[0];
    let text_content = content_item
        .as_text()
        .expect("Tool result content should be text");

    serde_json::from_str(&text_content.text).expect("Tool result should be valid JSON")
}

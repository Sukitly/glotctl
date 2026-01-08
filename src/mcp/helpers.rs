//! Helper functions for MCP server operations.

use std::path::Path;

use crate::json_writer::JsonWriter;

use super::types::{KeyActionDetail, LocaleError, LocaleResult};

/// Process a single locale translation entry.
///
/// Returns `Ok(LocaleResult)` on success, `Err(LocaleError)` on failure.
/// The caller can convert `LocaleError` to `LocaleResult` using `LocaleError::into_result()`.
pub fn process_locale_translation(
    translation: &serde_json::Value,
    messages_dir: &Path,
) -> Result<LocaleResult, LocaleError> {
    // Extract locale
    let locale = translation
        .get("locale")
        .and_then(|v| v.as_str())
        .ok_or_else(|| LocaleError::new("unknown", "Missing 'locale' field"))?;

    // Validate locale name
    validate_locale_name(locale).map_err(|msg| LocaleError::new(locale, msg))?;

    // Extract keys
    let keys = translation
        .get("keys")
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            LocaleError::new(
                locale,
                "Missing or invalid 'keys' field (must be an object)",
            )
        })?;

    // Validate keys is not empty
    if keys.is_empty() {
        return Err(LocaleError::new(locale, "Keys cannot be empty"));
    }

    let file_path = messages_dir.join(format!("{}.json", locale));

    // Open or create the locale file
    let mut writer = JsonWriter::open_or_create(&file_path)
        .map_err(|e| LocaleError::new(locale, format!("Failed to open locale file: {}", e)))?;

    let mut added_count = 0;
    let mut updated_count = 0;
    let mut details = Vec::new();

    for (key, value) in keys {
        // Validate key format
        validate_key_format(key)
            .map_err(|msg| LocaleError::new(locale, format!("Invalid key '{}': {}", key, msg)))?;

        // Validate and convert value
        let json_value = validate_and_convert_value(value).map_err(|msg| {
            LocaleError::new(locale, format!("Invalid value for key '{}': {}", key, msg))
        })?;

        let action = writer.add_value(key, json_value);
        match action {
            crate::json_writer::KeyAction::Added => added_count += 1,
            crate::json_writer::KeyAction::Updated => updated_count += 1,
        }

        details.push(KeyActionDetail {
            key: key.to_string(),
            action: action.as_str().to_string(),
        });
    }

    // Save the file
    writer
        .save()
        .map_err(|e| LocaleError::new(locale, format!("Failed to save locale file: {}", e)))?;

    Ok(LocaleResult {
        locale: locale.to_string(),
        success: true,
        file_path: Some(file_path.to_string_lossy().to_string()),
        added_count: Some(added_count),
        updated_count: Some(updated_count),
        details: Some(details),
        error: None,
    })
}

/// Validate locale name for security and correctness.
///
/// Only allows alphanumeric characters, hyphens, and underscores.
/// This prevents path traversal attacks and ensures safe file creation.
pub fn validate_locale_name(locale: &str) -> Result<(), String> {
    if locale.is_empty() {
        return Err("Locale name cannot be empty".to_string());
    }
    // Only allow alphanumeric, hyphen, and underscore
    // This is a strict allowlist approach for security
    if !locale
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Locale name can only contain letters, numbers, hyphens, and underscores".to_string(),
        );
    }
    Ok(())
}

/// Validate key format for security and correctness.
///
/// Keys must:
/// - Not be empty
/// - Not start or end with '.'
/// - Not contain consecutive dots '..'
/// - Only contain alphanumeric characters, dots, underscores, and hyphens
/// - Not contain whitespace or control characters
pub fn validate_key_format(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("Key cannot be empty".to_string());
    }
    if key.starts_with('.') {
        return Err("Key cannot start with '.'".to_string());
    }
    if key.ends_with('.') {
        return Err("Key cannot end with '.'".to_string());
    }
    if key.contains("..") {
        return Err("Key cannot contain consecutive dots '..'".to_string());
    }
    // Security: reject whitespace and control characters, only allow safe charset
    // Allowed: alphanumeric, dots (for nesting), underscores, hyphens
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err(
            "Key can only contain letters, numbers, dots, underscores, and hyphens".to_string(),
        );
    }
    Ok(())
}

/// Validate and convert value to serde_json::Value
pub fn validate_and_convert_value(value: &serde_json::Value) -> Result<serde_json::Value, String> {
    match value {
        serde_json::Value::String(_) => Ok(value.clone()),
        serde_json::Value::Array(arr) => {
            // Validate all elements are strings
            for (i, item) in arr.iter().enumerate() {
                if !item.is_string() {
                    return Err(format!("Array element at index {} is not a string", i));
                }
            }
            Ok(value.clone())
        }
        _ => Err("Value must be a string or an array of strings".to_string()),
    }
}

/// Parse missing locales from details string.
/// Format: "(value) missing in: de, fr, ja"
///
/// # Note
/// TODO: This implementation is fragile as it parses a human-readable string format.
/// Any upstream change to the details wording (e.g., changing "missing in:" to
/// "not found in:") will silently break locale extraction. For MCP server use cases,
/// consider using structured fields instead of parsing formatted strings.
pub fn parse_missing_locales(details: &str) -> Vec<String> {
    if let Some(pos) = details.find("missing in:") {
        details[pos + "missing in:".len()..]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    // ============================================================
    // Locale Name Validation Tests
    // ============================================================

    #[test]
    fn test_validate_locale_name_valid() {
        assert!(validate_locale_name("en").is_ok());
        assert!(validate_locale_name("zh-CN").is_ok());
        assert!(validate_locale_name("pt-BR").is_ok());
        assert!(validate_locale_name("en_US").is_ok());
    }

    #[test]
    fn test_validate_locale_name_empty() {
        let result = validate_locale_name("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_locale_name_with_slash() {
        let result = validate_locale_name("en/US");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("letters, numbers, hyphens"));
    }

    #[test]
    fn test_validate_locale_name_with_backslash() {
        let result = validate_locale_name("en\\US");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("letters, numbers, hyphens"));
    }

    #[test]
    fn test_validate_locale_name_with_double_dots() {
        let result = validate_locale_name("..en");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("letters, numbers, hyphens"));
    }

    #[test]
    fn test_validate_locale_name_path_traversal() {
        let result = validate_locale_name("../../../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("letters, numbers, hyphens"));
    }

    #[test]
    fn test_validate_locale_name_with_spaces() {
        let result = validate_locale_name("en US");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("letters, numbers, hyphens"));
    }

    // ============================================================
    // Key Format Validation Tests
    // ============================================================

    #[test]
    fn test_validate_key_format_valid() {
        assert!(validate_key_format("Common.submit").is_ok());
        assert!(validate_key_format("a.b.c").is_ok());
        assert!(validate_key_format("singleKey").is_ok());
        assert!(validate_key_format("Auth.login.button").is_ok());
    }

    #[test]
    fn test_validate_key_format_empty() {
        let result = validate_key_format("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_key_format_starts_with_dot() {
        let result = validate_key_format(".key");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("start with '.'"));
    }

    #[test]
    fn test_validate_key_format_ends_with_dot() {
        let result = validate_key_format("key.");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("end with '.'"));
    }

    #[test]
    fn test_validate_key_format_consecutive_dots() {
        let result = validate_key_format("a..b");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("consecutive dots"));
    }

    // ============================================================
    // Value Validation Tests
    // ============================================================

    #[test]
    fn test_validate_value_string() {
        let value = json!("Hello World");
        let result = validate_and_convert_value(&value);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), json!("Hello World"));
    }

    #[test]
    fn test_validate_value_string_array() {
        let value = json!(["item1", "item2", "item3"]);
        let result = validate_and_convert_value(&value);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), json!(["item1", "item2", "item3"]));
    }

    #[test]
    fn test_validate_value_number_rejected() {
        let value = json!(42);
        let result = validate_and_convert_value(&value);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be a string or an array"));
    }

    #[test]
    fn test_validate_value_boolean_rejected() {
        let value = json!(true);
        let result = validate_and_convert_value(&value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_value_object_rejected() {
        let value = json!({"nested": "object"});
        let result = validate_and_convert_value(&value);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_value_mixed_array_rejected() {
        let value = json!(["string", 123, "another"]);
        let result = validate_and_convert_value(&value);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a string"));
    }

    #[test]
    fn test_validate_value_empty_array() {
        let value = json!([]);
        let result = validate_and_convert_value(&value);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), json!([]));
    }

    // ============================================================
    // Process Locale Translation Tests
    // ============================================================

    #[test]
    fn test_process_locale_single_key_string() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en",
            "keys": {
                "Common.submit": "Submit"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_ok());

        let locale_result = result.unwrap();
        assert!(locale_result.success);
        assert_eq!(locale_result.locale, "en");
        assert_eq!(locale_result.added_count, Some(1));
        assert_eq!(locale_result.updated_count, Some(0));

        // Verify file content
        let file_path = messages_dir.join("en.json");
        let content = fs::read_to_string(file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["Common"]["submit"], "Submit");
    }

    #[test]
    fn test_process_locale_single_key_array() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en",
            "keys": {
                "Common.items": ["Item 1", "Item 2"]
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_ok());

        let locale_result = result.unwrap();
        assert!(locale_result.success);
        assert_eq!(locale_result.added_count, Some(1));

        // Verify file content
        let file_path = messages_dir.join("en.json");
        let content = fs::read_to_string(file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["Common"]["items"][0], "Item 1");
        assert_eq!(parsed["Common"]["items"][1], "Item 2");
    }

    #[test]
    fn test_process_locale_multiple_keys() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en",
            "keys": {
                "Common.submit": "Submit",
                "Common.cancel": "Cancel",
                "Auth.login": "Login"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_ok());

        let locale_result = result.unwrap();
        assert!(locale_result.success);
        assert_eq!(locale_result.added_count, Some(3));
        assert_eq!(locale_result.updated_count, Some(0));
    }

    #[test]
    fn test_process_locale_nested_key_creation() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en",
            "keys": {
                "a.b.c.d": "Deep nested value"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_ok());

        // Verify file content
        let file_path = messages_dir.join("en.json");
        let content = fs::read_to_string(file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["a"]["b"]["c"]["d"], "Deep nested value");
    }

    #[test]
    fn test_process_locale_update_existing_key() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        // Create initial file
        let file_path = messages_dir.join("en.json");
        fs::write(&file_path, r#"{"Common": {"submit": "Old Value"}}"#).unwrap();

        let translation = json!({
            "locale": "en",
            "keys": {
                "Common.submit": "New Value"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_ok());

        let locale_result = result.unwrap();
        assert!(locale_result.success);
        assert_eq!(locale_result.added_count, Some(0));
        assert_eq!(locale_result.updated_count, Some(1));

        // Verify file content
        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["Common"]["submit"], "New Value");
    }

    #[test]
    fn test_process_locale_preserve_existing_keys() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        // Create initial file with existing keys
        let file_path = messages_dir.join("en.json");
        fs::write(
            &file_path,
            r#"{"Common": {"existing": "Keep Me", "another": "Also Keep"}}"#,
        )
        .unwrap();

        let translation = json!({
            "locale": "en",
            "keys": {
                "Common.newKey": "New Value"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_ok());

        // Verify existing keys are preserved
        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["Common"]["existing"], "Keep Me");
        assert_eq!(parsed["Common"]["another"], "Also Keep");
        assert_eq!(parsed["Common"]["newKey"], "New Value");
    }

    #[test]
    fn test_process_locale_invalid_locale_with_slash() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en/US",
            "keys": {
                "key": "value"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.locale, "en/US");
        assert!(err.message.contains("letters, numbers, hyphens"));
    }

    #[test]
    fn test_process_locale_invalid_locale_path_traversal() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "../invalid",
            "keys": {
                "key": "value"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_err());

        let err = result.unwrap_err();
        // Now uses allowlist validation
        assert!(err.message.contains("letters, numbers, hyphens"));
    }

    #[test]
    fn test_process_locale_empty_keys() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en",
            "keys": {}
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.locale, "en");
        assert!(err.message.contains("cannot be empty"));
    }

    #[test]
    fn test_process_locale_invalid_key_starts_with_dot() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en",
            "keys": {
                ".key": "value"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message.contains("start with '.'"));
    }

    #[test]
    fn test_process_locale_invalid_value_number() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en",
            "keys": {
                "key": 123
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message.contains("Invalid value"));
    }

    #[test]
    fn test_process_locale_invalid_value_mixed_array() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en",
            "keys": {
                "key": ["string", 123]
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message.contains("not a string"));
    }

    #[test]
    fn test_process_locale_missing_locale_field() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "keys": {
                "key": "value"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.locale, "unknown");
        assert!(err.message.contains("Missing 'locale'"));
    }

    #[test]
    fn test_process_locale_missing_keys_field() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        let translation = json!({
            "locale": "en"
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message.contains("Missing or invalid 'keys'"));
    }

    #[test]
    fn test_process_locale_creates_new_file() {
        let dir = tempdir().unwrap();
        let messages_dir = dir.path().to_path_buf();

        // Ensure the file doesn't exist
        let file_path = messages_dir.join("new-locale.json");
        assert!(!file_path.exists());

        let translation = json!({
            "locale": "new-locale",
            "keys": {
                "key": "value"
            }
        });

        let result = process_locale_translation(&translation, &messages_dir);
        assert!(result.is_ok());

        // Verify file was created
        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["key"], "value");
    }

    // ============================================================
    // Parse Missing Locales Tests
    // ============================================================

    #[test]
    fn test_parse_missing_locales_basic() {
        let result = parse_missing_locales("(\"hello\") missing in: de, fr, ja");
        assert_eq!(result, vec!["de", "fr", "ja"]);
    }

    #[test]
    fn test_parse_missing_locales_single() {
        let result = parse_missing_locales("(\"value\") missing in: zh-CN");
        assert_eq!(result, vec!["zh-CN"]);
    }

    #[test]
    fn test_parse_missing_locales_no_match() {
        let result = parse_missing_locales("some other format");
        assert!(result.is_empty());
    }
}

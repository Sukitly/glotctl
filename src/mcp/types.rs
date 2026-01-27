use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================
// Tool Input Parameters
// ============================================================

/// Parameters for scan_hardcoded tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanHardcodedParams {
    /// Path to the project root directory
    pub project_root_path: String,
    /// Maximum number of items to return (default: 20, max: 100)
    #[serde(default)]
    pub limit: Option<u64>,
    /// Number of items to skip (default: 0)
    #[serde(default)]
    pub offset: Option<u64>,
}

/// Parameters for scan_overview tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanOverviewParams {
    /// Path to the project root directory
    pub project_root_path: String,
}

/// Parameters for scan_primary_missing tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanPrimaryMissingParams {
    /// Path to the project root directory
    pub project_root_path: String,
    /// Maximum number of items to return (default: 50, max: 100)
    #[serde(default)]
    pub limit: Option<u64>,
    /// Number of items to skip (default: 0)
    #[serde(default)]
    pub offset: Option<u64>,
}

/// Parameters for scan_replica_lag tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanReplicaLagParams {
    /// Path to the project root directory
    pub project_root_path: String,
    /// Maximum number of items to return (default: 50, max: 100)
    #[serde(default)]
    pub limit: Option<u64>,
    /// Number of items to skip (default: 0)
    #[serde(default)]
    pub offset: Option<u64>,
}

/// Parameters for scan_untranslated tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanUntranslatedParams {
    /// Path to the project root directory
    pub project_root_path: String,
    /// Maximum number of items to return (default: 50, max: 100)
    #[serde(default)]
    pub limit: Option<u64>,
    /// Number of items to skip (default: 0)
    #[serde(default)]
    pub offset: Option<u64>,
}

/// Parameters for scan_type_mismatch tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanTypeMismatchParams {
    /// Path to the project root directory
    pub project_root_path: String,
    /// Maximum number of items to return (default: 50, max: 100)
    #[serde(default)]
    pub limit: Option<u64>,
    /// Number of items to skip (default: 0)
    #[serde(default)]
    pub offset: Option<u64>,
}

/// Parameters for get_locales tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetLocalesParams {
    /// Path to the project root directory
    pub project_root_path: String,
}

/// Parameters for get_config tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetConfigParams {
    /// Path to the project root directory
    pub project_root_path: String,
}

/// Parameters for add_translations tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddTranslationsParams {
    /// Path to the project root directory
    pub project_root_path: String,
    /// Array of translations to add
    pub translations: Vec<TranslationEntry>,
}

/// A translation entry for a single locale
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TranslationEntry {
    /// Locale code (e.g., "en", "zh-CN")
    pub locale: String,
    /// Key-value pairs to add. Supports nested keys (e.g., "common.title") and string arrays.
    pub keys: serde_json::Map<String, serde_json::Value>,
}

// ============================================================
// Config Types (get_config)
// ============================================================

/// Configuration DTO for MCP
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDto {
    /// True if config was loaded from a file, false if using defaults
    pub from_file: bool,
    pub config: ConfigValues,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigValues {
    pub ignores: Vec<String>,
    pub includes: Vec<String>,
    pub checked_attributes: Vec<String>,
    pub ignore_texts: Vec<String>,
    pub messages_root: String,
    pub primary_locale: String,
    pub source_root: String,
    pub ignore_test_files: bool,
}

impl From<crate::config::Config> for ConfigValues {
    fn from(c: crate::config::Config) -> Self {
        Self {
            ignores: c.ignores,
            includes: c.includes,
            checked_attributes: c.checked_attributes,
            ignore_texts: c.ignore_texts,
            messages_root: c.messages_root,
            primary_locale: c.primary_locale,
            source_root: c.source_root,
            ignore_test_files: c.ignore_test_files,
        }
    }
}

// ============================================================
// Locales Types (get_locales)
// ============================================================

/// Result of get_locales operation
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocalesResult {
    pub messages_dir: String,
    pub primary_locale: String,
    pub locales: Vec<LocaleInfo>,
}

/// Information about a single locale
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocaleInfo {
    pub locale: String,
    pub file_path: String,
    pub key_count: usize,
}

// ============================================================
// AddTranslations Types (add_translations)
// ============================================================

/// Result of add_translations operation (batch)
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddTranslationsResult {
    /// True if all locales were processed successfully
    pub success: bool,
    /// Results for each locale
    pub results: Vec<LocaleResult>,
    /// Summary statistics
    pub summary: AddTranslationsSummary,
}

/// Result for a single locale
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocaleResult {
    /// The locale code (e.g., "en", "zh-CN")
    pub locale: String,
    /// Whether this locale was processed successfully
    pub success: bool,
    /// Path to the locale file (only on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Number of keys added (only on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_count: Option<usize>,
    /// Number of keys updated (only on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_count: Option<usize>,
    /// Details of each key action (only on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<KeyActionDetail>>,
    /// Error message (only on failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Summary statistics for the batch operation
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddTranslationsSummary {
    /// Total number of locales in the request
    pub total_locales: usize,
    /// Number of locales processed successfully
    pub successful_locales: usize,
    /// Number of locales that failed
    pub failed_locales: usize,
    /// Total keys added across all locales
    pub total_keys_added: usize,
    /// Total keys updated across all locales
    pub total_keys_updated: usize,
}

/// Detail of each key action
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeyActionDetail {
    pub key: String,
    pub action: String, // "added", "updated"
}

/// Error type for locale processing operations.
///
/// This separates the error case from the success case, making the control flow
/// clearer than using `Result<LocaleResult, LocaleResult>`.
#[derive(Debug)]
pub struct LocaleError {
    /// The locale code (may be "unknown" if locale field was missing)
    pub locale: String,
    /// Error message describing what went wrong
    pub message: String,
}

impl LocaleError {
    pub fn new(locale: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            locale: locale.into(),
            message: message.into(),
        }
    }

    /// Convert this error into a LocaleResult for serialization
    pub fn into_result(self) -> LocaleResult {
        LocaleResult {
            locale: self.locale,
            success: false,
            file_path: None,
            added_count: None,
            updated_count: None,
            details: None,
            error: Some(self.message),
        }
    }
}

// ============================================================
// Hardcoded Scan Types (scan_hardcoded)
// ============================================================

/// Result of scan_hardcoded operation
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HardcodedScanResult {
    pub total_count: usize,
    pub total_file_count: usize,
    pub items: Vec<HardcodedItem>,
    pub pagination: Pagination,
}

/// A single hardcoded text issue
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HardcodedItem {
    pub file_path: String,
    pub line: usize,
    pub col: usize,
    pub text: String,
    pub source_line: String,
}

// ============================================================
// Scan Overview Types (scan_overview)
// ============================================================

/// Result of scan_overview operation - statistics only
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScanOverviewResult {
    pub hardcoded: HardcodedStats,
    pub primary_missing: PrimaryMissingStats,
    pub replica_lag: ReplicaLagStats,
    pub untranslated: UntranslatedStats,
    pub type_mismatch: TypeMismatchStats,
}

/// Statistics for hardcoded text issues
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HardcodedStats {
    pub total_count: usize,
    pub file_count: usize,
}

/// Statistics for primary locale missing keys
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrimaryMissingStats {
    pub total_count: usize,
}

/// Statistics for replica lag issues
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaLagStats {
    pub total_count: usize,
    pub affected_locales: Vec<String>,
}

/// Statistics for untranslated values
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UntranslatedStats {
    pub total_count: usize,
    pub affected_locales: Vec<String>,
}

/// Statistics for type mismatch issues
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeMismatchStats {
    pub total_count: usize,
    pub affected_locales: Vec<String>,
}

// ============================================================
// Primary Missing Scan Types (scan_primary_missing)
// ============================================================

/// Result of scan_primary_missing operation
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrimaryMissingScanResult {
    pub total_count: usize,
    pub items: Vec<PrimaryMissingItem>,
    pub pagination: Pagination,
}

/// A key used in code but missing from primary locale
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrimaryMissingItem {
    pub key: String,
    pub file_path: String,
    pub line: usize,
    /// Source of the key if from dynamic key resolution (e.g., "from \"FEATURE_KEYS\"")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

// ============================================================
// Common Types for Key Usages
// ============================================================

/// Location where a translation key is used in code.
/// Used by replica-lag and untranslated scan results.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeyUsageLocation {
    pub file_path: String,
    pub line: usize,
    pub col: usize,
}

// ============================================================
// Replica Lag Scan Types (scan_replica_lag)
// ============================================================

/// Result of scan_replica_lag operation
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaLagScanResult {
    pub total_count: usize,
    pub items: Vec<ReplicaLagItem>,
    pub pagination: Pagination,
}

/// A key that exists in primary locale but missing in others
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaLagItem {
    pub key: String,
    pub value: String,
    /// Primary locale file path where this key is defined
    pub file_path: String,
    /// Line number in primary locale file
    pub line: usize,
    pub exists_in: String,
    pub missing_in: Vec<String>,
    /// Locations where this key is used in code (max 3)
    pub usages: Vec<KeyUsageLocation>,
    /// Total number of usages (may be more than usages.len())
    pub total_usages: usize,
}

// ============================================================
// Untranslated Scan Types (scan_untranslated)
// ============================================================

/// Result of scan_untranslated operation
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UntranslatedScanResult {
    pub total_count: usize,
    pub items: Vec<UntranslatedItem>,
    pub pagination: Pagination,
}

/// A value that is identical to primary locale (possibly not translated)
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UntranslatedItem {
    pub key: String,
    pub value: String,
    /// Primary locale file path where this key is defined
    pub file_path: String,
    /// Line number in primary locale file
    pub line: usize,
    /// Locales where the value is identical to primary locale
    pub identical_in: Vec<String>,
    pub primary_locale: String,
    /// Locations where this key is used in code (max 3)
    pub usages: Vec<KeyUsageLocation>,
    /// Total number of usages (may be more than usages.len())
    pub total_usages: usize,
}

// ============================================================
// Type Mismatch Scan Types (scan_type_mismatch)
// ============================================================

/// Result of scan_type_mismatch operation
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeMismatchScanResult {
    pub total_count: usize,
    pub items: Vec<TypeMismatchItem>,
    pub pagination: Pagination,
}

/// A value with type mismatch between primary and replica locales
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeMismatchItem {
    /// Full expanded key path
    pub key: String,
    /// Expected type in primary locale (e.g., "array", "string")
    pub expected_type: String,
    /// Primary locale file path
    pub primary_file_path: String,
    /// Line number in primary locale file
    pub primary_line: usize,
    /// Primary locale code
    pub primary_locale: String,
    /// Locales with mismatched types
    pub mismatched_in: Vec<TypeMismatchLocale>,
    /// Locations where this key is used in code (max 3)
    pub usages: Vec<KeyUsageLocation>,
    /// Total number of usages (may be more than usages.len())
    pub total_usages: usize,
}

/// Information about a locale with mismatched type
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeMismatchLocale {
    pub locale: String,
    pub actual_type: String,
    pub file_path: String,
    pub line: usize,
}

// ============================================================
// Common Types
// ============================================================

/// Pagination information
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use crate::mcp::types::*;
    use serde_json::json;

    // ============================================================
    // Parameter Deserialization Tests
    // ============================================================

    #[test]
    fn test_scan_hardcoded_params_full() {
        let json = json!({
            "project_root_path": "/path/to/project",
            "limit": 50,
            "offset": 10
        });
        let params: ScanHardcodedParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/path/to/project");
        assert_eq!(params.limit, Some(50));
        assert_eq!(params.offset, Some(10));
    }

    #[test]
    fn test_scan_hardcoded_params_minimal() {
        let json = json!({
            "project_root_path": "/path/to/project"
        });
        let params: ScanHardcodedParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/path/to/project");
        assert_eq!(params.limit, None);
        assert_eq!(params.offset, None);
    }

    #[test]
    fn test_scan_hardcoded_params_missing_required() {
        let json = json!({
            "limit": 50
        });
        let result: Result<ScanHardcodedParams, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_overview_params() {
        let json = json!({
            "project_root_path": "/my/project"
        });
        let params: ScanOverviewParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/my/project");
    }

    #[test]
    fn test_scan_primary_missing_params() {
        let json = json!({
            "project_root_path": "/path",
            "limit": 100,
            "offset": 25
        });
        let params: ScanPrimaryMissingParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/path");
        assert_eq!(params.limit, Some(100));
        assert_eq!(params.offset, Some(25));
    }

    #[test]
    fn test_scan_replica_lag_params() {
        let json = json!({
            "project_root_path": "/path"
        });
        let params: ScanReplicaLagParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/path");
        assert_eq!(params.limit, None);
        assert_eq!(params.offset, None);
    }

    #[test]
    fn test_scan_untranslated_params() {
        let json = json!({
            "project_root_path": "/path",
            "limit": 30
        });
        let params: ScanUntranslatedParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/path");
        assert_eq!(params.limit, Some(30));
        assert_eq!(params.offset, None);
    }

    #[test]
    fn test_scan_type_mismatch_params() {
        let json = json!({
            "project_root_path": "/path",
            "limit": 30
        });
        let params: ScanTypeMismatchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/path");
        assert_eq!(params.limit, Some(30));
        assert_eq!(params.offset, None);
    }

    #[test]
    fn test_get_locales_params() {
        let json = json!({
            "project_root_path": "/locales/path"
        });
        let params: GetLocalesParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/locales/path");
    }

    #[test]
    fn test_get_config_params() {
        let json = json!({
            "project_root_path": "/config/path"
        });
        let params: GetConfigParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/config/path");
    }

    #[test]
    fn test_add_translations_params() {
        let json = json!({
            "project_root_path": "/path",
            "translations": [
                {"locale": "en", "keys": {"title": "Hello", "desc": "World"}},
                {"locale": "zh-CN", "keys": {"title": "你好"}}
            ]
        });
        let params: AddTranslationsParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.project_root_path, "/path");
        assert_eq!(params.translations.len(), 2);
        assert_eq!(params.translations[0].locale, "en");
        assert_eq!(params.translations[0].keys.len(), 2);
        assert_eq!(params.translations[1].locale, "zh-CN");
        assert_eq!(params.translations[1].keys.len(), 1);
    }

    #[test]
    fn test_add_translations_params_empty_translations() {
        let json = json!({
            "project_root_path": "/path",
            "translations": []
        });
        let params: AddTranslationsParams = serde_json::from_value(json).unwrap();
        assert!(params.translations.is_empty());
    }

    #[test]
    fn test_translation_entry_with_array_value() {
        let json = json!({
            "locale": "en",
            "keys": {"items": ["one", "two", "three"]}
        });
        let entry: TranslationEntry = serde_json::from_value(json).unwrap();
        assert_eq!(entry.locale, "en");
        assert!(entry.keys["items"].is_array());
    }

    // ============================================================
    // JSON Schema Validation Tests
    // ============================================================

    #[test]
    fn test_scan_overview_params_schema_has_required_field() {
        let schema = schemars::schema_for!(ScanOverviewParams);
        let schema_json = serde_json::to_value(&schema).unwrap();

        // Verify project_root_path is in required array
        let required = schema_json["required"].as_array().unwrap();
        assert!(required.contains(&json!("project_root_path")));

        // Verify property exists
        assert!(schema_json["properties"]["project_root_path"].is_object());
    }

    #[test]
    fn test_scan_hardcoded_params_schema_optional_fields() {
        let schema = schemars::schema_for!(ScanHardcodedParams);
        let schema_json = serde_json::to_value(&schema).unwrap();

        // Verify project_root_path is required
        let required = schema_json["required"].as_array().unwrap();
        assert!(required.contains(&json!("project_root_path")));

        // Verify limit and offset are NOT required (optional fields)
        assert!(!required.contains(&json!("limit")));
        assert!(!required.contains(&json!("offset")));

        // But they should exist as properties
        assert!(schema_json["properties"]["limit"].is_object());
        assert!(schema_json["properties"]["offset"].is_object());
    }

    #[test]
    fn test_add_translations_params_schema() {
        let schema = schemars::schema_for!(AddTranslationsParams);
        let schema_json = serde_json::to_value(&schema).unwrap();

        // Verify both fields are required
        let required = schema_json["required"].as_array().unwrap();
        assert!(required.contains(&json!("project_root_path")));
        assert!(required.contains(&json!("translations")));

        // Verify translations is an array type
        let translations_prop = &schema_json["properties"]["translations"];
        assert!(translations_prop.is_object());
    }

    #[test]
    fn test_translation_entry_schema() {
        let schema = schemars::schema_for!(TranslationEntry);
        let schema_json = serde_json::to_value(&schema).unwrap();

        // Verify both locale and keys are required
        let required = schema_json["required"].as_array().unwrap();
        assert!(required.contains(&json!("locale")));
        assert!(required.contains(&json!("keys")));
    }

    #[test]
    fn test_all_params_have_project_root_path_required() {
        // Ensure all parameter types have project_root_path as required
        let schemas = vec![
            schemars::schema_for!(ScanHardcodedParams),
            schemars::schema_for!(ScanOverviewParams),
            schemars::schema_for!(ScanPrimaryMissingParams),
            schemars::schema_for!(ScanReplicaLagParams),
            schemars::schema_for!(ScanUntranslatedParams),
            schemars::schema_for!(GetLocalesParams),
            schemars::schema_for!(GetConfigParams),
            schemars::schema_for!(AddTranslationsParams),
        ];

        for schema in schemas {
            let schema_json = serde_json::to_value(&schema).unwrap();
            let required = schema_json["required"].as_array().unwrap();
            assert!(
                required.contains(&json!("project_root_path")),
                "Schema missing required project_root_path: {:?}",
                schema_json
            );
        }
    }
}

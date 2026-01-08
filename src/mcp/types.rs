use schemars::JsonSchema;
use serde::Serialize;

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
    pub messages_dir: String,
    pub primary_locale: String,
    pub ignore_test_files: bool,
}

impl From<crate::config::Config> for ConfigValues {
    fn from(c: crate::config::Config) -> Self {
        Self {
            ignores: c.ignores,
            includes: c.includes,
            checked_attributes: c.checked_attributes,
            ignore_texts: c.ignore_texts,
            messages_dir: c.messages_dir,
            primary_locale: c.primary_locale,
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
    pub exists_in: String,
    pub missing_in: Vec<String>,
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

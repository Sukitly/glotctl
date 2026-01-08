//! Type definitions for missing key detection.

use crate::checkers::schema::SchemaCallInfo;
use crate::checkers::value_source::ResolvedKey;
use crate::parsers::comment::PatternWarning;

/// Result of scanning a file for missing translation keys.
#[derive(Debug, Default)]
pub struct MissingKeyResult {
    pub used_keys: Vec<UsedKey>,
    pub warnings: Vec<DynamicKeyWarning>,
    pub schema_calls: Vec<SchemaCallInfo>,
    /// Resolved keys from ValueAnalyzer
    pub resolved_keys: Vec<ResolvedKey>,
    /// Warnings from glot-message-keys annotation parsing
    pub pattern_warnings: Vec<PatternWarning>,
}

/// A translation key used in code.
#[derive(Debug, Clone)]
pub struct UsedKey {
    pub full_key: String,
    pub file_path: String,
    pub line: usize,
    pub col: usize,
    pub source_line: String,
}

/// Reason why a key is considered dynamic.
#[derive(Debug, Clone)]
pub enum DynamicKeyReason {
    /// Key is a variable: t(keyName)
    VariableKey,
    /// Key is a template with expressions: t(`${prefix}.key`)
    TemplateWithExpr,
}

/// Warning about a dynamic key that cannot be statically analyzed.
#[derive(Debug, Clone)]
pub struct DynamicKeyWarning {
    pub file_path: String,
    pub line: usize,
    pub col: usize,
    pub reason: DynamicKeyReason,
    pub source_line: String,
    /// Suggested pattern hint for the user (e.g., "genderOptions.*")
    pub hint: Option<String>,
}

/// Stores glot-message-keys annotation data for a line.
#[derive(Debug, Clone)]
pub(super) struct GlotAnnotation {
    /// Keys after glob expansion (without namespace prefix)
    pub keys: Vec<String>,
}

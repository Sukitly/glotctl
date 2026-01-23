//! Key extraction result types.
//!
//! Contains the result types for translation key extraction:
//! - `KeyExtractionResult`: the main result struct
//! - `UsedKey`: a static translation key found in code
//! - `DynamicKeyWarning`: warning about unresolvable dynamic keys
//! - `DynamicKeyReason`: why a key is considered dynamic

use crate::extraction::resolver::ResolvedKey;
use crate::extraction::schema::SchemaCallInfo;
use crate::parsers::comment::PatternWarning;

/// Result of extracting translation keys from a single file.
#[derive(Debug, Default)]
pub struct KeyExtractionResult {
    /// Static translation keys found in code.
    pub used_keys: Vec<UsedKey>,
    /// Warnings about dynamic keys that cannot be statically analyzed.
    pub warnings: Vec<DynamicKeyWarning>,
    /// Schema function calls that generate translation keys.
    pub schema_calls: Vec<SchemaCallInfo>,
    /// Resolved keys from ValueAnalyzer (for dynamic key resolution).
    pub resolved_keys: Vec<ResolvedKey>,
    /// Warnings from glot-message-keys annotation parsing.
    pub pattern_warnings: Vec<PatternWarning>,
}

/// A translation key used in code.
#[derive(Debug, Clone)]
pub struct UsedKey {
    /// The full key including namespace (e.g., "Common.submit").
    pub full_key: String,
    /// File path where the key was found.
    pub file_path: String,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (1-based).
    pub col: usize,
    /// The source line content.
    pub source_line: String,
    /// Whether the key usage is in JSX context (for comment style).
    pub in_jsx_context: bool,
    /// Whether the untranslated rule is disabled for this usage.
    pub untranslated_disabled: bool,
}

/// Reason why a key is considered dynamic.
#[derive(Debug, Clone)]
pub enum DynamicKeyReason {
    /// Key is a variable: `t(keyName)`
    VariableKey,
    /// Key is a template with expressions: `t(\`${prefix}.key\`)`
    TemplateWithExpr,
}

/// Warning about a dynamic key that cannot be statically analyzed.
#[derive(Debug, Clone)]
pub struct DynamicKeyWarning {
    /// File path where the warning occurred.
    pub file_path: String,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (1-based).
    pub col: usize,
    /// Why the key is considered dynamic.
    pub reason: DynamicKeyReason,
    /// The source line content.
    pub source_line: String,
    /// Suggested pattern hint for the user (formatted message).
    pub hint: Option<String>,
    /// The raw pattern inferred from template (e.g., "Common.*.submit").
    /// Used by fix command to generate glot-message-keys comments.
    pub pattern: Option<String>,
    /// Whether this warning occurred in JSX context.
    /// Used to determine comment style in hints.
    pub in_jsx_context: bool,
}

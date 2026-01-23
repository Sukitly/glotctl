//! Key extraction result types.
//!
//! Contains the result types for translation key extraction:
//! - `KeyExtractionResult`: the main result struct (final output)
//! - `RawFileResult`: raw extraction before comment resolution (Phase 2 output)
//! - `FinalFileResult`: final result after comment resolution (Phase 3 output)
//! - `UsedKey`: a static translation key found in code
//! - `DynamicKeyWarning`: warning about unresolvable dynamic keys
//! - `DynamicKeyReason`: why a key is considered dynamic

use crate::extraction::extract::{ResolvedKey, ValueSource};
use crate::extraction::resolve::comments::parser::PatternWarning;
use crate::extraction::schema::SchemaCallInfo;
use crate::issue::{HardcodedIssue, SourceLocation};

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

// ============================================================
// Three-Phase Pipeline Types
// ============================================================

/// Hardcoded text candidate before comment resolution (Phase 2 output).
///
/// This represents potentially hardcoded text that will be filtered by
/// disable directives in Phase 3 (resolution).
#[derive(Debug, Clone)]
pub struct HardcodedCandidate {
    /// Location in source code.
    pub location: SourceLocation,
    /// The hardcoded text content.
    pub text: String,
    /// Source line for context.
    pub source_line: Option<String>,
}

impl From<HardcodedCandidate> for HardcodedIssue {
    fn from(candidate: HardcodedCandidate) -> Self {
        HardcodedIssue {
            location: candidate.location,
            text: candidate.text,
            source_line: candidate.source_line,
        }
    }
}

/// Translation function call information for dynamic key resolution (Phase 2 output).
///
/// This represents a `t(...)` call where the argument is a dynamic expression
/// that needs annotation resolution in Phase 3.
#[derive(Debug, Clone)]
pub struct TranslationCall {
    /// File path where the call occurred.
    pub file_path: String,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (1-based).
    pub col: usize,
    /// Source line content.
    pub source_line: String,
    /// The translation source (namespace info).
    pub namespace: Option<String>,
    /// The analyzed value source of the translation key argument.
    pub arg_source: ValueSource,
    /// Whether this call is in JSX context (for comment style).
    pub in_jsx_context: bool,
}

/// Raw extraction result before comment resolution (Phase 2 output).
///
/// This is the output of the extraction phase, containing all detected patterns
/// before applying disable directives and glot-message-keys annotations.
#[derive(Debug, Default)]
pub struct RawFileResult {
    /// All hardcoded candidates (not yet filtered by disable directives).
    pub hardcoded_candidates: Vec<HardcodedCandidate>,
    /// All translation calls that need annotation resolution.
    pub translation_calls: Vec<TranslationCall>,
    /// Static keys (already resolved, just need untranslated_disabled flag).
    pub used_keys: Vec<UsedKey>,
    /// Schema function calls.
    pub schema_calls: Vec<SchemaCallInfo>,
    /// Resolved keys from ValueAnalyzer.
    pub resolved_keys: Vec<ResolvedKey>,
}

/// Final file result after comment resolution (Phase 3 output).
///
/// This combines hardcoded issues and extraction results, ready for rule checking.
#[derive(Debug)]
pub struct FinalFileResult {
    /// Hardcoded issues (after filtering by disable directives).
    pub hardcoded_issues: Vec<HardcodedIssue>,
    /// Translation key extraction result (after annotation resolution).
    pub extraction: KeyExtractionResult,
}

//! Issue types for i18n analysis results.
//!
//! This module defines the core issue types using an enum + trait pattern:
//! - Each issue type is a separate struct with its specific fields
//! - The `Issue` enum wraps all issue types
//! - The `IssueReport` trait provides a common interface for CLI output
//! - Each issue type can have its own methods (e.g., `to_mcp_item()` for MCP conversion)
//!
//! ## Location Types
//!
//! Two distinct location types are used for type safety:
//! - `SourceLocation`: For issues in source code (TSX/JSX), includes `in_jsx_context`
//! - `MessageLocation`: For issues in message files (JSON), no JSX context

use std::cmp::Ordering;
use std::fmt;

use enum_dispatch::enum_dispatch;

use crate::mcp::types::{HardcodedItem, KeyUsageLocation, ReplicaLagItem, UntranslatedItem};

// ============================================================
// Constants
// ============================================================

/// Maximum number of usage locations to include in issues.
/// Shared across modules for consistency.
pub const MAX_KEY_USAGES: usize = 3;

// ============================================================
// Location Types
// ============================================================

/// Location in source code files (TSX/JSX/TS/JS).
///
/// Used for issues found in application code where JSX context matters
/// for determining comment style (`//` vs `{/* */}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file_path: String,
    pub line: usize,
    pub col: Option<usize>,
    /// Whether this location is in JSX children context.
    /// Determines comment style: JS `//` vs JSX `{/* */}`
    pub in_jsx_context: bool,
}

impl SourceLocation {
    pub fn new(file_path: impl Into<String>, line: usize) -> Self {
        Self {
            file_path: file_path.into(),
            line,
            col: None,
            in_jsx_context: false,
        }
    }

    pub fn with_col(mut self, col: usize) -> Self {
        self.col = Some(col);
        self
    }

    pub fn with_jsx_context(mut self, in_jsx: bool) -> Self {
        self.in_jsx_context = in_jsx;
        self
    }

    /// Get column with default value (for cases where col is required).
    pub fn col_or_default(&self) -> usize {
        self.col.unwrap_or(1)
    }
}

// Manual Ord implementation - excludes in_jsx_context from ordering
impl Ord for SourceLocation {
    fn cmp(&self, other: &Self) -> Ordering {
        self.file_path
            .cmp(&other.file_path)
            .then_with(|| self.line.cmp(&other.line))
            .then_with(|| self.col.cmp(&other.col))
    }
}

impl PartialOrd for SourceLocation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Location in message/locale files (JSON).
///
/// Used for issues found in translation files.
/// Does not have JSX context since JSON files are not JSX.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageLocation {
    pub file_path: String,
    pub line: usize,
    pub col: Option<usize>,
}

impl MessageLocation {
    pub fn new(file_path: impl Into<String>, line: usize) -> Self {
        Self {
            file_path: file_path.into(),
            line,
            col: None,
        }
    }

    pub fn with_col(mut self, col: usize) -> Self {
        self.col = Some(col);
        self
    }

    /// Get column with default value (for cases where col is required).
    pub fn col_or_default(&self) -> usize {
        self.col.unwrap_or(1)
    }
}

// ============================================================
// KeyUsage - uses SourceLocation
// ============================================================

/// Represents a location where a translation key is used in source code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyUsage {
    pub location: SourceLocation,
}

impl KeyUsage {
    pub fn new(location: SourceLocation) -> Self {
        Self { location }
    }

    // Convenience accessors
    pub fn file_path(&self) -> &str {
        &self.location.file_path
    }

    pub fn line(&self) -> usize {
        self.location.line
    }

    pub fn col(&self) -> usize {
        self.location.col_or_default()
    }

    pub fn in_jsx_context(&self) -> bool {
        self.location.in_jsx_context
    }

    /// Convert to MCP response type.
    pub fn to_mcp_location(&self) -> KeyUsageLocation {
        KeyUsageLocation {
            file_path: self.location.file_path.clone(),
            line: self.location.line,
            col: self.location.col_or_default(),
        }
    }
}

// ============================================================
// Severity and Rule
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    HardcodedText,
    MissingKey,
    DynamicKey,
    ReplicaLag,
    UnusedKey,
    OrphanKey,
    UntrackedNamespace,
    ParseError,
    Untranslated,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rule::HardcodedText => write!(f, "hardcoded-text"),
            Rule::MissingKey => write!(f, "missing-key"),
            Rule::DynamicKey => write!(f, "dynamic-key"),
            Rule::ReplicaLag => write!(f, "replica-lag"),
            Rule::UnusedKey => write!(f, "unused-key"),
            Rule::OrphanKey => write!(f, "orphan-key"),
            Rule::UntrackedNamespace => write!(f, "untracked-namespace"),
            Rule::ParseError => write!(f, "parse-error"),
            Rule::Untranslated => write!(f, "untranslated"),
        }
    }
}

// ============================================================
// Issue Types with SourceLocation (found in source code)
// ============================================================

/// Hardcoded text in JSX/TSX that should use translations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardcodedIssue {
    pub location: SourceLocation,
    pub text: String,
    pub source_line: Option<String>,
}

impl HardcodedIssue {
    /// Convert to MCP response type.
    pub fn to_mcp_item(&self) -> HardcodedItem {
        HardcodedItem {
            file_path: self.location.file_path.clone(),
            line: self.location.line,
            col: self.location.col_or_default(),
            text: self.text.clone(),
            source_line: self.source_line.clone().unwrap_or_default(),
        }
    }
}

/// Translation key used in code but missing from primary locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingKeyIssue {
    pub location: SourceLocation,
    pub key: String,
    pub source_line: Option<String>,
    /// If from schema validation: (schema_name, schema_file)
    pub from_schema: Option<(String, String)>,
}

/// Dynamic key that cannot be statically analyzed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicKeyIssue {
    pub location: SourceLocation,
    pub reason: String,
    pub source_line: Option<String>,
    pub hint: Option<String>,
}

/// Namespace could not be determined for schema-derived key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrackedNamespaceIssue {
    pub location: SourceLocation,
    pub raw_key: String,
    pub schema_name: String,
    pub source_line: Option<String>,
}

/// Some candidates from dynamic key source are missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingDynamicKeyCandidatesIssue {
    pub location: SourceLocation,
    pub source_object: String,
    pub missing_keys: Vec<String>,
    pub source_line: Option<String>,
    /// Cached message for display (owned version to satisfy lifetime)
    #[doc(hidden)]
    message_cache: String,
}

impl MissingDynamicKeyCandidatesIssue {
    pub fn new(
        location: SourceLocation,
        source_object: String,
        missing_keys: Vec<String>,
        source_line: Option<String>,
    ) -> Self {
        let message_cache = format!("dynamic key from \"{}\"", source_object);
        Self {
            location,
            source_object,
            missing_keys,
            source_line,
            message_cache,
        }
    }
}

// ============================================================
// Issue Types with MessageLocation (found in JSON files)
// ============================================================

/// Key exists in primary locale but missing in other locales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaLagIssue {
    pub location: MessageLocation,
    pub key: String,
    pub value: String,
    pub primary_locale: String,
    pub missing_in: Vec<String>,
    pub usages: Vec<KeyUsage>,
    pub total_usages: usize,
}

impl ReplicaLagIssue {
    /// Convert to MCP response type.
    pub fn to_mcp_item(&self) -> ReplicaLagItem {
        ReplicaLagItem {
            key: self.key.clone(),
            value: self.value.clone(),
            file_path: self.location.file_path.clone(),
            line: self.location.line,
            exists_in: self.primary_locale.clone(),
            missing_in: self.missing_in.clone(),
            usages: self.usages.iter().map(KeyUsage::to_mcp_location).collect(),
            total_usages: self.total_usages,
        }
    }
}

/// Key defined in locale files but not used in code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusedKeyIssue {
    pub location: MessageLocation,
    pub key: String,
    pub value: String,
}

/// Key exists in non-primary locale but not in primary locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanKeyIssue {
    pub location: MessageLocation,
    pub key: String,
    pub value: String,
    pub locale: String,
}

/// Value is identical to primary locale (possibly not translated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntranslatedIssue {
    pub location: MessageLocation,
    pub key: String,
    pub value: String,
    pub primary_locale: String,
    pub identical_in: Vec<String>,
    pub usages: Vec<KeyUsage>,
    pub total_usages: usize,
}

impl UntranslatedIssue {
    /// Convert to MCP response type.
    pub fn to_mcp_item(&self) -> UntranslatedItem {
        UntranslatedItem {
            key: self.key.clone(),
            value: self.value.clone(),
            file_path: self.location.file_path.clone(),
            line: self.location.line,
            primary_locale: self.primary_locale.clone(),
            identical_in: self.identical_in.clone(),
            usages: self.usages.iter().map(KeyUsage::to_mcp_location).collect(),
            total_usages: self.total_usages,
        }
    }
}

// ============================================================
// Special Issue Types
// ============================================================

/// File could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseErrorIssue {
    pub file_path: String,
    pub error: String,
}

// ============================================================
// IssueReport Trait (for CLI output)
// ============================================================

/// Common interface for all issue types, used for CLI output.
#[enum_dispatch]
pub trait IssueReport {
    /// File path where issue was found.
    fn file_path(&self) -> Option<&str>;

    /// Line number (1-based).
    fn line(&self) -> Option<usize>;

    /// Column number (1-based).
    fn col(&self) -> Option<usize>;

    /// Primary message (key name or hardcoded text).
    fn message(&self) -> &str;

    /// Severity level.
    fn severity(&self) -> Severity;

    /// Rule type.
    fn rule(&self) -> Rule;

    /// Source code line for context.
    fn source_line(&self) -> Option<&str>;

    /// Hint for fixing the issue.
    fn hint(&self) -> Option<&str>;

    /// Format details for CLI output (the "= note:" line).
    fn format_details(&self) -> Option<String>;

    /// Get usages if this issue type tracks them.
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)>;
}

// ============================================================
// Issue Enum (wraps all issue types)
// ============================================================

/// An i18n issue found during analysis.
#[enum_dispatch(IssueReport)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    Hardcoded(HardcodedIssue),
    MissingKey(MissingKeyIssue),
    DynamicKey(DynamicKeyIssue),
    ReplicaLag(ReplicaLagIssue),
    UnusedKey(UnusedKeyIssue),
    OrphanKey(OrphanKeyIssue),
    Untranslated(UntranslatedIssue),
    UntrackedNamespace(UntrackedNamespaceIssue),
    MissingDynamicKeyCandidates(MissingDynamicKeyCandidatesIssue),
    ParseError(ParseErrorIssue),
}

// ============================================================
// IssueReport Implementations for Each Issue Type
// ============================================================

impl IssueReport for HardcodedIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.text
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn rule(&self) -> Rule {
        Rule::HardcodedText
    }
    fn source_line(&self) -> Option<&str> {
        self.source_line.as_deref()
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        None
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        None
    }
}

impl IssueReport for MissingKeyIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.key
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn rule(&self) -> Rule {
        Rule::MissingKey
    }
    fn source_line(&self) -> Option<&str> {
        self.source_line.as_deref()
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        self.from_schema
            .as_ref()
            .map(|(name, file)| format!("from {} ({})", name, file))
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        None
    }
}

impl IssueReport for DynamicKeyIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.reason
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn rule(&self) -> Rule {
        Rule::DynamicKey
    }
    fn source_line(&self) -> Option<&str> {
        self.source_line.as_deref()
    }
    fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
    fn format_details(&self) -> Option<String> {
        None
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        None
    }
}

impl IssueReport for ReplicaLagIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.key
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn rule(&self) -> Rule {
        Rule::ReplicaLag
    }
    fn source_line(&self) -> Option<&str> {
        None
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        Some(format!(
            "(\"{}\") missing in: {}",
            self.value,
            self.missing_in.join(", ")
        ))
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        Some((&self.usages, self.total_usages))
    }
}

impl IssueReport for UnusedKeyIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.key
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn rule(&self) -> Rule {
        Rule::UnusedKey
    }
    fn source_line(&self) -> Option<&str> {
        None
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        Some(format!("(\"{}\")", self.value))
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        None
    }
}

impl IssueReport for OrphanKeyIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.key
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn rule(&self) -> Rule {
        Rule::OrphanKey
    }
    fn source_line(&self) -> Option<&str> {
        None
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        Some(format!("in {} (\"{}\")", self.locale, self.value))
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        None
    }
}

impl IssueReport for UntranslatedIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.key
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn rule(&self) -> Rule {
        Rule::Untranslated
    }
    fn source_line(&self) -> Option<&str> {
        None
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        Some(format!(
            "(\"{}\") identical in: {}",
            self.value,
            self.identical_in.join(", ")
        ))
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        Some((&self.usages, self.total_usages))
    }
}

impl IssueReport for UntrackedNamespaceIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.raw_key
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn rule(&self) -> Rule {
        Rule::UntrackedNamespace
    }
    fn source_line(&self) -> Option<&str> {
        self.source_line.as_deref()
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        Some(format!(
            "from {} - namespace could not be determined",
            self.schema_name
        ))
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        None
    }
}

impl IssueReport for MissingDynamicKeyCandidatesIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.location.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(self.location.line)
    }
    fn col(&self) -> Option<usize> {
        self.location.col
    }
    fn message(&self) -> &str {
        &self.message_cache
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn rule(&self) -> Rule {
        Rule::MissingKey
    }
    fn source_line(&self) -> Option<&str> {
        self.source_line.as_deref()
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        Some(format!("missing: {}", self.missing_keys.join(", ")))
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        None
    }
}

impl IssueReport for ParseErrorIssue {
    fn file_path(&self) -> Option<&str> {
        Some(&self.file_path)
    }
    fn line(&self) -> Option<usize> {
        Some(1)
    }
    fn col(&self) -> Option<usize> {
        Some(1)
    }
    fn message(&self) -> &str {
        &self.error
    }
    fn severity(&self) -> Severity {
        Severity::Error
    }
    fn rule(&self) -> Rule {
        Rule::ParseError
    }
    fn source_line(&self) -> Option<&str> {
        None
    }
    fn hint(&self) -> Option<&str> {
        None
    }
    fn format_details(&self) -> Option<String> {
        None
    }
    fn usages(&self) -> Option<(&Vec<KeyUsage>, usize)> {
        None
    }
}

// ============================================================
// Ord Implementation for Issue (for sorting)
// ============================================================

impl Ord for Issue {
    fn cmp(&self, other: &Self) -> Ordering {
        // Sort by: file_path (None last), line, col, message
        //
        // Note: message comparison is needed for deterministic ordering because:
        // - HashMap iteration order is non-deterministic
        // - Multiple issues can have same file_path/line/col (e.g., unused keys in same JSON file)
        // - Without message comparison, test output would be flaky
        match (self.file_path(), other.file_path()) {
            (Some(a), Some(b)) => a
                .cmp(b)
                .then_with(|| self.line().cmp(&other.line()))
                .then_with(|| self.col().cmp(&other.col()))
                .then_with(|| self.message().cmp(other.message())),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => self
                .rule()
                .cmp(&other.rule())
                .then_with(|| self.message().cmp(other.message())),
        }
    }
}

impl PartialOrd for Issue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // SourceLocation Tests
    // ============================================================

    #[test]
    fn test_source_location_builder() {
        let loc = SourceLocation::new("./src/app.tsx", 10)
            .with_col(5)
            .with_jsx_context(true);
        assert_eq!(loc.file_path, "./src/app.tsx");
        assert_eq!(loc.line, 10);
        assert_eq!(loc.col, Some(5));
        assert!(loc.in_jsx_context);
    }

    #[test]
    fn test_source_location_col_or_default() {
        let loc_with_col = SourceLocation::new("./src/app.tsx", 10).with_col(5);
        assert_eq!(loc_with_col.col_or_default(), 5);

        let loc_without_col = SourceLocation::new("./src/app.tsx", 10);
        assert_eq!(loc_without_col.col_or_default(), 1);
    }

    #[test]
    fn test_source_location_ordering_ignores_jsx_context() {
        let loc1 = SourceLocation::new("./a.tsx", 10)
            .with_col(5)
            .with_jsx_context(true);
        let loc2 = SourceLocation::new("./a.tsx", 10)
            .with_col(5)
            .with_jsx_context(false);

        // Should be equal despite different in_jsx_context
        assert_eq!(loc1.cmp(&loc2), Ordering::Equal);
    }

    // ============================================================
    // MessageLocation Tests
    // ============================================================

    #[test]
    fn test_message_location_builder() {
        let loc = MessageLocation::new("./messages/en.json", 5).with_col(3);
        assert_eq!(loc.file_path, "./messages/en.json");
        assert_eq!(loc.line, 5);
        assert_eq!(loc.col, Some(3));
    }

    #[test]
    fn test_message_location_col_or_default() {
        let loc_with_col = MessageLocation::new("./messages/en.json", 5).with_col(3);
        assert_eq!(loc_with_col.col_or_default(), 3);

        let loc_without_col = MessageLocation::new("./messages/en.json", 5);
        assert_eq!(loc_without_col.col_or_default(), 1);
    }

    // ============================================================
    // KeyUsage Tests
    // ============================================================

    #[test]
    fn test_key_usage_new() {
        let loc = SourceLocation::new("./src/Button.tsx", 25)
            .with_col(10)
            .with_jsx_context(true);
        let usage = KeyUsage::new(loc);

        assert_eq!(usage.file_path(), "./src/Button.tsx");
        assert_eq!(usage.line(), 25);
        assert_eq!(usage.col(), 10);
        assert!(usage.in_jsx_context());
    }

    #[test]
    fn test_key_usage_to_mcp_location() {
        let loc = SourceLocation::new("./src/Button.tsx", 25)
            .with_col(10)
            .with_jsx_context(true);
        let usage = KeyUsage::new(loc);

        let mcp_loc = usage.to_mcp_location();
        assert_eq!(mcp_loc.file_path, "./src/Button.tsx");
        assert_eq!(mcp_loc.line, 25);
        assert_eq!(mcp_loc.col, 10);
    }

    // ============================================================
    // HardcodedIssue Tests
    // ============================================================

    #[test]
    fn test_hardcoded_issue_report() {
        let issue = HardcodedIssue {
            location: SourceLocation::new("./src/app.tsx", 10)
                .with_col(5)
                .with_jsx_context(false),
            text: "Hello".to_string(),
            source_line: Some("const x = \"Hello\";".to_string()),
        };

        assert_eq!(issue.file_path(), Some("./src/app.tsx"));
        assert_eq!(issue.line(), Some(10));
        assert_eq!(issue.col(), Some(5));
        assert_eq!(issue.message(), "Hello");
        assert_eq!(issue.severity(), Severity::Error);
        assert_eq!(issue.rule(), Rule::HardcodedText);
        assert_eq!(issue.source_line(), Some("const x = \"Hello\";"));
        assert!(issue.hint().is_none());
        assert!(issue.format_details().is_none());
        assert!(issue.usages().is_none());
    }

    #[test]
    fn test_hardcoded_issue_jsx_context_accessible() {
        let issue = HardcodedIssue {
            location: SourceLocation::new("./src/app.tsx", 10)
                .with_col(5)
                .with_jsx_context(true),
            text: "Hello".to_string(),
            source_line: None,
        };

        // in_jsx_context is accessible through location
        assert!(issue.location.in_jsx_context);
    }

    #[test]
    fn test_hardcoded_to_mcp_item() {
        let issue = HardcodedIssue {
            location: SourceLocation::new("./src/app.tsx", 10)
                .with_col(5)
                .with_jsx_context(false),
            text: "Hello".to_string(),
            source_line: Some("const x = \"Hello\";".to_string()),
        };

        let item = issue.to_mcp_item();
        assert_eq!(item.file_path, "./src/app.tsx");
        assert_eq!(item.line, 10);
        assert_eq!(item.col, 5);
        assert_eq!(item.text, "Hello");
        assert_eq!(item.source_line, "const x = \"Hello\";");
    }

    // ============================================================
    // ReplicaLagIssue Tests
    // ============================================================

    #[test]
    fn test_replica_lag_issue_report() {
        let issue = ReplicaLagIssue {
            location: MessageLocation::new("./messages/en.json", 5),
            key: "Common.submit".to_string(),
            value: "Submit".to_string(),
            primary_locale: "en".to_string(),
            missing_in: vec!["zh".to_string(), "ja".to_string()],
            usages: vec![KeyUsage::new(
                SourceLocation::new("./src/Button.tsx", 25)
                    .with_col(10)
                    .with_jsx_context(true),
            )],
            total_usages: 3,
        };

        assert_eq!(issue.file_path(), Some("./messages/en.json"));
        assert_eq!(issue.line(), Some(5));
        assert_eq!(issue.col(), None);
        assert_eq!(issue.message(), "Common.submit");
        assert_eq!(issue.severity(), Severity::Error);
        assert_eq!(issue.rule(), Rule::ReplicaLag);
        assert_eq!(
            issue.format_details(),
            Some("(\"Submit\") missing in: zh, ja".to_string())
        );

        let (usages, total) = issue.usages().unwrap();
        assert_eq!(usages.len(), 1);
        assert_eq!(total, 3);
        assert!(usages[0].in_jsx_context()); // KeyUsage has SourceLocation
    }

    #[test]
    fn test_replica_lag_to_mcp_item() {
        let issue = ReplicaLagIssue {
            location: MessageLocation::new("./messages/en.json", 5),
            key: "Common.submit".to_string(),
            value: "Submit".to_string(),
            primary_locale: "en".to_string(),
            missing_in: vec!["zh".to_string()],
            usages: vec![KeyUsage::new(
                SourceLocation::new("./src/Button.tsx", 25)
                    .with_col(10)
                    .with_jsx_context(true),
            )],
            total_usages: 1,
        };

        let item = issue.to_mcp_item();
        assert_eq!(item.key, "Common.submit");
        assert_eq!(item.value, "Submit");
        assert_eq!(item.file_path, "./messages/en.json");
        assert_eq!(item.line, 5);
        assert_eq!(item.exists_in, "en");
        assert_eq!(item.missing_in, vec!["zh"]);
        assert_eq!(item.usages.len(), 1);
        assert_eq!(item.total_usages, 1);
    }

    // ============================================================
    // UntranslatedIssue Tests
    // ============================================================

    #[test]
    fn test_untranslated_issue_report() {
        let issue = UntranslatedIssue {
            location: MessageLocation::new("./messages/en.json", 5),
            key: "Common.submit".to_string(),
            value: "Submit".to_string(),
            primary_locale: "en".to_string(),
            identical_in: vec!["zh".to_string(), "ja".to_string()],
            usages: vec![],
            total_usages: 0,
        };

        assert_eq!(issue.message(), "Common.submit");
        assert_eq!(issue.severity(), Severity::Warning);
        assert_eq!(issue.rule(), Rule::Untranslated);
        assert_eq!(
            issue.format_details(),
            Some("(\"Submit\") identical in: zh, ja".to_string())
        );
    }

    #[test]
    fn test_untranslated_to_mcp_item() {
        let issue = UntranslatedIssue {
            location: MessageLocation::new("./messages/en.json", 5),
            key: "Common.submit".to_string(),
            value: "Submit".to_string(),
            primary_locale: "en".to_string(),
            identical_in: vec!["zh".to_string(), "ja".to_string()],
            usages: vec![],
            total_usages: 0,
        };

        let item = issue.to_mcp_item();
        assert_eq!(item.key, "Common.submit");
        assert_eq!(item.value, "Submit");
        assert_eq!(item.primary_locale, "en");
        assert_eq!(item.identical_in, vec!["zh", "ja"]);
    }

    // ============================================================
    // UnusedKeyIssue Tests
    // ============================================================

    #[test]
    fn test_unused_key_issue_report() {
        let issue = UnusedKeyIssue {
            location: MessageLocation::new("./messages/en.json", 5),
            key: "Common.unused".to_string(),
            value: "Unused".to_string(),
        };

        assert_eq!(issue.message(), "Common.unused");
        assert_eq!(issue.severity(), Severity::Warning);
        assert_eq!(issue.rule(), Rule::UnusedKey);
        assert_eq!(issue.format_details(), Some("(\"Unused\")".to_string()));
    }

    // ============================================================
    // OrphanKeyIssue Tests
    // ============================================================

    #[test]
    fn test_orphan_key_issue_report() {
        let issue = OrphanKeyIssue {
            location: MessageLocation::new("./messages/zh.json", 5),
            key: "Common.orphan".to_string(),
            value: "孤儿".to_string(),
            locale: "zh".to_string(),
        };

        assert_eq!(issue.message(), "Common.orphan");
        assert_eq!(issue.severity(), Severity::Warning);
        assert_eq!(issue.rule(), Rule::OrphanKey);
        assert_eq!(issue.format_details(), Some("in zh (\"孤儿\")".to_string()));
    }

    // ============================================================
    // MissingKeyIssue Tests
    // ============================================================

    #[test]
    fn test_missing_key_issue_report() {
        let issue = MissingKeyIssue {
            location: SourceLocation::new("./src/Button.tsx", 15).with_col(10),
            key: "Common.submit".to_string(),
            source_line: Some("const label = t('Common.submit');".to_string()),
            from_schema: None,
        };

        assert_eq!(issue.file_path(), Some("./src/Button.tsx"));
        assert_eq!(issue.line(), Some(15));
        assert_eq!(issue.col(), Some(10));
        assert_eq!(issue.message(), "Common.submit");
        assert_eq!(issue.severity(), Severity::Error);
        assert_eq!(issue.rule(), Rule::MissingKey);
        assert_eq!(
            issue.source_line(),
            Some("const label = t('Common.submit');")
        );
        assert!(issue.hint().is_none());
        assert!(issue.format_details().is_none());
        assert!(issue.usages().is_none());
    }

    #[test]
    fn test_missing_key_issue_with_schema() {
        let issue = MissingKeyIssue {
            location: SourceLocation::new("./src/Form.tsx", 20).with_col(5),
            key: "Form.email".to_string(),
            source_line: None,
            from_schema: Some((
                "formSchema".to_string(),
                "./src/schemas/form.ts".to_string(),
            )),
        };

        assert_eq!(issue.message(), "Form.email");
        assert_eq!(
            issue.format_details(),
            Some("from formSchema (./src/schemas/form.ts)".to_string())
        );
    }

    // ============================================================
    // DynamicKeyIssue Tests
    // ============================================================

    #[test]
    fn test_dynamic_key_issue_report() {
        let issue = DynamicKeyIssue {
            location: SourceLocation::new("./src/utils.tsx", 30).with_col(12),
            reason: "dynamic key".to_string(),
            source_line: Some("const msg = t(keyVar);".to_string()),
            hint: None,
        };

        assert_eq!(issue.file_path(), Some("./src/utils.tsx"));
        assert_eq!(issue.line(), Some(30));
        assert_eq!(issue.col(), Some(12));
        assert_eq!(issue.message(), "dynamic key");
        assert_eq!(issue.severity(), Severity::Warning);
        assert_eq!(issue.rule(), Rule::DynamicKey);
        assert_eq!(issue.source_line(), Some("const msg = t(keyVar);"));
        assert!(issue.hint().is_none());
        assert!(issue.format_details().is_none());
        assert!(issue.usages().is_none());
    }

    #[test]
    fn test_dynamic_key_issue_with_hint() {
        let issue = DynamicKeyIssue {
            location: SourceLocation::new("./src/app.tsx", 25).with_col(8),
            reason: "template with expression".to_string(),
            source_line: Some("t(`prefix.${key}`)".to_string()),
            hint: Some("Consider using a key object pattern".to_string()),
        };

        assert_eq!(issue.message(), "template with expression");
        assert_eq!(issue.hint(), Some("Consider using a key object pattern"));
    }

    // ============================================================
    // ParseErrorIssue Tests
    // ============================================================

    #[test]
    fn test_parse_error_issue_report() {
        let issue = ParseErrorIssue {
            file_path: "./src/broken.tsx".to_string(),
            error: "Unexpected token at line 5".to_string(),
        };

        assert_eq!(issue.file_path(), Some("./src/broken.tsx"));
        assert_eq!(issue.line(), Some(1)); // Always returns 1
        assert_eq!(issue.col(), Some(1)); // Always returns 1
        assert_eq!(issue.message(), "Unexpected token at line 5");
        assert_eq!(issue.severity(), Severity::Error);
        assert_eq!(issue.rule(), Rule::ParseError);
        assert!(issue.source_line().is_none());
        assert!(issue.hint().is_none());
        assert!(issue.format_details().is_none());
        assert!(issue.usages().is_none());
    }

    // ============================================================
    // UntrackedNamespaceIssue Tests
    // ============================================================

    #[test]
    fn test_untracked_namespace_issue_report() {
        let issue = UntrackedNamespaceIssue {
            location: SourceLocation::new("./src/dynamic.tsx", 40).with_col(15),
            raw_key: "someKey".to_string(),
            schema_name: "dynamicSchema".to_string(),
            source_line: Some("t(schema.someKey)".to_string()),
        };

        assert_eq!(issue.file_path(), Some("./src/dynamic.tsx"));
        assert_eq!(issue.line(), Some(40));
        assert_eq!(issue.col(), Some(15));
        assert_eq!(issue.message(), "someKey");
        assert_eq!(issue.severity(), Severity::Warning);
        assert_eq!(issue.rule(), Rule::UntrackedNamespace);
        assert_eq!(issue.source_line(), Some("t(schema.someKey)"));
        assert!(issue.hint().is_none());
        assert_eq!(
            issue.format_details(),
            Some("from dynamicSchema - namespace could not be determined".to_string())
        );
        assert!(issue.usages().is_none());
    }

    // ============================================================
    // MissingDynamicKeyCandidatesIssue Tests
    // ============================================================

    #[test]
    fn test_missing_dynamic_key_candidates_issue_report() {
        let issue = MissingDynamicKeyCandidatesIssue::new(
            SourceLocation::new("./src/app.tsx", 50).with_col(20),
            "FEATURE_KEYS".to_string(),
            vec!["features.alpha".to_string(), "features.beta".to_string()],
            Some("FEATURE_KEYS.map(k => t(k))".to_string()),
        );

        assert_eq!(issue.file_path(), Some("./src/app.tsx"));
        assert_eq!(issue.line(), Some(50));
        assert_eq!(issue.col(), Some(20));
        assert_eq!(issue.message(), "dynamic key from \"FEATURE_KEYS\"");
        assert_eq!(issue.severity(), Severity::Error);
        assert_eq!(issue.rule(), Rule::MissingKey);
        assert_eq!(issue.source_line(), Some("FEATURE_KEYS.map(k => t(k))"));
        assert!(issue.hint().is_none());
        assert_eq!(
            issue.format_details(),
            Some("missing: features.alpha, features.beta".to_string())
        );
        assert!(issue.usages().is_none());
    }

    // ============================================================
    // Issue Enum Tests
    // ============================================================

    #[test]
    fn test_issue_enum_delegates_to_inner() {
        let inner = HardcodedIssue {
            location: SourceLocation::new("./src/app.tsx", 10)
                .with_col(5)
                .with_jsx_context(true),
            text: "Hello".to_string(),
            source_line: None,
        };
        let issue = Issue::Hardcoded(inner);

        // IssueReport methods should delegate to inner type
        assert_eq!(issue.file_path(), Some("./src/app.tsx"));
        assert_eq!(issue.line(), Some(10));
        assert_eq!(issue.col(), Some(5));
        assert_eq!(issue.message(), "Hello");
        assert_eq!(issue.severity(), Severity::Error);
        assert_eq!(issue.rule(), Rule::HardcodedText);
    }

    #[test]
    fn test_issue_sorting() {
        let issue1 = Issue::Hardcoded(HardcodedIssue {
            location: SourceLocation::new("./a.tsx", 10)
                .with_col(5)
                .with_jsx_context(false),
            text: "A".to_string(),
            source_line: None,
        });
        let issue2 = Issue::Hardcoded(HardcodedIssue {
            location: SourceLocation::new("./a.tsx", 10)
                .with_col(10)
                .with_jsx_context(false),
            text: "B".to_string(),
            source_line: None,
        });
        let issue3 = Issue::Hardcoded(HardcodedIssue {
            location: SourceLocation::new("./b.tsx", 5)
                .with_col(1)
                .with_jsx_context(false),
            text: "C".to_string(),
            source_line: None,
        });

        let mut issues = [issue3.clone(), issue1.clone(), issue2.clone()];
        issues.sort();

        // Should be sorted by file_path, then line, then col
        assert_eq!(issues[0].message(), "A");
        assert_eq!(issues[1].message(), "B");
        assert_eq!(issues[2].message(), "C");
    }

    #[test]
    fn test_issue_sorting_mixed_types() {
        // Test sorting across different issue types
        let hardcoded = Issue::Hardcoded(HardcodedIssue {
            location: SourceLocation::new("./a.tsx", 10)
                .with_col(5)
                .with_jsx_context(false),
            text: "Hello".to_string(),
            source_line: None,
        });

        let missing = Issue::MissingKey(MissingKeyIssue {
            location: SourceLocation::new("./a.tsx", 5).with_col(1), // Same file, earlier line
            key: "some.key".to_string(),
            source_line: None,
            from_schema: None,
        });

        let unused = Issue::UnusedKey(UnusedKeyIssue {
            location: MessageLocation::new("./b.json", 1), // Different file
            key: "unused.key".to_string(),
            value: "Unused".to_string(),
        });

        let mut issues = [hardcoded.clone(), unused.clone(), missing.clone()];
        issues.sort();

        // Should be sorted by file_path, then line
        assert_eq!(issues[0].message(), "some.key"); // ./a.tsx:5
        assert_eq!(issues[1].message(), "Hello"); // ./a.tsx:10
        assert_eq!(issues[2].message(), "unused.key"); // ./b.json:1
    }

    // ============================================================
    // Edge Case Tests
    // ============================================================

    #[test]
    fn test_format_details_with_special_chars() {
        // Test that special characters in values are preserved correctly
        let issue = ReplicaLagIssue {
            location: MessageLocation::new("./messages/en.json", 5),
            key: "Common.greeting".to_string(),
            value: "Hello \"World\" (test)".to_string(), // Contains quotes and parens
            primary_locale: "en".to_string(),
            missing_in: vec!["zh".to_string()],
            usages: vec![],
            total_usages: 0,
        };

        // The format_details should preserve the special characters
        assert_eq!(
            issue.format_details(),
            Some("(\"Hello \"World\" (test)\") missing in: zh".to_string())
        );
    }
}

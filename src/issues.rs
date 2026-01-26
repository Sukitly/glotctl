//! Issue types for i18n analysis results.
//!
//! This module defines all issue types that can be detected during i18n analysis.
//! Each issue is self-contained with all information needed by:
//! - Reporter: to display the issue to users (CLI, MCP, etc.)
//! - Action: to fix the issue (insert comments, delete keys, etc.)

use enum_dispatch::enum_dispatch;

use crate::core::ResolvedKeyUsage;
use crate::core::{LocaleTypeMismatch, MessageContext, SourceContext, ValueType};

// ============================================================
// Severity and Rule
// ============================================================

/// Severity level of an issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
        }
    }
}

/// Rule identifier for each issue type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    HardcodedText,
    MissingKey,
    UnresolvedKey,
    ReplicaLag,
    UnusedKey,
    OrphanKey,
    Untranslated,
    TypeMismatch,
    ParseError,
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Use short names for directive compatibility with v1
            Rule::HardcodedText => write!(f, "hardcoded"),
            Rule::MissingKey => write!(f, "missing-key"),
            Rule::UnresolvedKey => write!(f, "unresolved-key"),
            Rule::ReplicaLag => write!(f, "replica-lag"),
            Rule::UnusedKey => write!(f, "unused-key"),
            Rule::OrphanKey => write!(f, "orphan-key"),
            Rule::Untranslated => write!(f, "untranslated"),
            Rule::TypeMismatch => write!(f, "type-mismatch"),
            Rule::ParseError => write!(f, "parse-error"),
        }
    }
}

// ============================================================
// Unresolved Key Reason
// ============================================================

/// Reason why a key cannot be resolved (statically analyzed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueUnresolvedKeyReason {
    /// Key is a variable: `t(keyName)`
    VariableKey,
    /// Key is a template with expressions: `t(\`${prefix}.key\`)`
    TemplateWithExpr,
    /// Namespace cannot be determined for schema-derived keys.
    /// Contains the schema function name.
    UnknownNamespace { schema_name: String },
}

impl std::fmt::Display for IssueUnresolvedKeyReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueUnresolvedKeyReason::VariableKey => write!(f, "variable key"),
            IssueUnresolvedKeyReason::TemplateWithExpr => write!(f, "template with expression"),
            IssueUnresolvedKeyReason::UnknownNamespace { schema_name } => {
                write!(f, "unknown namespace for schema '{}'", schema_name)
            }
        }
    }
}

// ============================================================
// Issue Types - Source Code (SourceContext)
// ============================================================

/// Hardcoded text in JSX/TSX that should use translations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardcodedTextIssue {
    pub context: SourceContext,
    /// The hardcoded text content.
    pub text: String,
}

impl HardcodedTextIssue {
    pub fn severity() -> Severity {
        Severity::Error
    }

    pub fn rule() -> Rule {
        Rule::HardcodedText
    }
}

/// Translation key used in code but missing from primary locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingKeyIssue {
    pub context: SourceContext,
    /// The missing translation key.
    pub key: String,
    /// If from schema validation: (schema_name, schema_file).
    pub from_schema: Option<(String, String)>,
}

impl MissingKeyIssue {
    pub fn severity() -> Severity {
        Severity::Error
    }

    pub fn rule() -> Rule {
        Rule::MissingKey
    }
}

/// Key that cannot be statically resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedKeyIssue {
    pub context: SourceContext,
    /// Why the key cannot be resolved.
    pub reason: IssueUnresolvedKeyReason,
    /// Hint for the user on how to fix.
    pub hint: Option<String>,
    /// Pattern for FixAction to generate glot-message-keys comment.
    pub pattern: Option<String>,
}

impl UnresolvedKeyIssue {
    pub fn severity() -> Severity {
        Severity::Warning
    }

    pub fn rule() -> Rule {
        Rule::UnresolvedKey
    }
}

// ============================================================
// Issue Types - Message Files (MessageContext)
// ============================================================

/// Key defined in locale files but not used in code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusedKeyIssue {
    pub context: MessageContext,
}

impl UnusedKeyIssue {
    pub fn severity() -> Severity {
        Severity::Warning
    }

    pub fn rule() -> Rule {
        Rule::UnusedKey
    }
}

/// Key exists in non-primary locale but not in primary locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanKeyIssue {
    pub context: MessageContext,
    /// The locale where this orphan key exists.
    pub locale: String,
}

impl OrphanKeyIssue {
    pub fn severity() -> Severity {
        Severity::Warning
    }

    pub fn rule() -> Rule {
        Rule::OrphanKey
    }
}

/// Key exists in primary locale but missing in other locales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaLagIssue {
    pub context: MessageContext,
    /// The primary locale code (e.g., "en").
    pub primary_locale: String,
    /// Locales where this key is missing.
    pub missing_in: Vec<String>,
    /// Locations where this key is used in code.
    pub usages: Vec<ResolvedKeyUsage>,
}

impl ReplicaLagIssue {
    pub fn severity() -> Severity {
        Severity::Error
    }

    pub fn rule() -> Rule {
        Rule::ReplicaLag
    }
}

/// Value is identical to primary locale (possibly not translated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntranslatedIssue {
    pub context: MessageContext,
    /// The primary locale code (e.g., "en").
    pub primary_locale: String,
    /// Locales where the value is identical to primary.
    pub identical_in: Vec<String>,
    /// Locations where this key is used in code.
    pub usages: Vec<ResolvedKeyUsage>,
}

impl UntranslatedIssue {
    pub fn severity() -> Severity {
        Severity::Warning
    }

    pub fn rule() -> Rule {
        Rule::Untranslated
    }
}

/// Value type mismatch between primary and replica locales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMismatchIssue {
    pub context: MessageContext,
    /// Expected type from primary locale.
    pub expected_type: ValueType,
    /// The primary locale code (e.g., "en").
    pub primary_locale: String,
    /// Locales with mismatched types.
    pub mismatched_in: Vec<LocaleTypeMismatch>,
    /// Locations where this key is used in code.
    pub usages: Vec<ResolvedKeyUsage>,
}

impl TypeMismatchIssue {
    pub fn severity() -> Severity {
        Severity::Error
    }

    pub fn rule() -> Rule {
        Rule::TypeMismatch
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

impl ParseErrorIssue {
    pub fn severity() -> Severity {
        Severity::Error
    }

    pub fn rule() -> Rule {
        Rule::ParseError
    }
}

// ============================================================
// Issue Enum
// ============================================================

/// An i18n issue found during analysis.
#[enum_dispatch(Report)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    HardcodedText(HardcodedTextIssue),
    MissingKey(MissingKeyIssue),
    UnresolvedKey(UnresolvedKeyIssue),
    UnusedKey(UnusedKeyIssue),
    OrphanKey(OrphanKeyIssue),
    ReplicaLag(ReplicaLagIssue),
    Untranslated(UntranslatedIssue),
    TypeMismatch(TypeMismatchIssue),
    ParseError(ParseErrorIssue),
}

impl Issue {
    pub fn severity(&self) -> Severity {
        match self {
            Issue::HardcodedText(_) => HardcodedTextIssue::severity(),
            Issue::MissingKey(_) => MissingKeyIssue::severity(),
            Issue::UnresolvedKey(_) => UnresolvedKeyIssue::severity(),
            Issue::UnusedKey(_) => UnusedKeyIssue::severity(),
            Issue::OrphanKey(_) => OrphanKeyIssue::severity(),
            Issue::ReplicaLag(_) => ReplicaLagIssue::severity(),
            Issue::Untranslated(_) => UntranslatedIssue::severity(),
            Issue::TypeMismatch(_) => TypeMismatchIssue::severity(),
            Issue::ParseError(_) => ParseErrorIssue::severity(),
        }
    }

    pub fn rule(&self) -> Rule {
        match self {
            Issue::HardcodedText(_) => HardcodedTextIssue::rule(),
            Issue::MissingKey(_) => MissingKeyIssue::rule(),
            Issue::UnresolvedKey(_) => UnresolvedKeyIssue::rule(),
            Issue::UnusedKey(_) => UnusedKeyIssue::rule(),
            Issue::OrphanKey(_) => OrphanKeyIssue::rule(),
            Issue::ReplicaLag(_) => ReplicaLagIssue::rule(),
            Issue::Untranslated(_) => UntranslatedIssue::rule(),
            Issue::TypeMismatch(_) => TypeMismatchIssue::rule(),
            Issue::ParseError(_) => ParseErrorIssue::rule(),
        }
    }
}

// ============================================================
// Report Trait (for CLI output)
// ============================================================

/// Location information for report output.
pub enum ReportLocation<'a> {
    /// Source code location (has source_line for context display).
    Source(&'a SourceContext),
    /// Message file location (no source_line, but has key/value).
    Message(&'a MessageContext),
    /// File-level only (for ParseError - no line context).
    File { path: &'a str },
}

/// Trait for types that can be reported to CLI.
///
/// This trait is implemented by all issue types to provide a consistent
/// interface for the report functions. Uses `enum_dispatch` for zero-cost
/// dispatch on the `Issue` enum.
#[enum_dispatch]
pub trait Report {
    /// Get the location for this issue.
    fn location(&self) -> ReportLocation<'_>;

    /// Primary message to display (key name, text, error, etc.).
    fn message(&self) -> String;

    /// Severity level.
    fn report_severity(&self) -> Severity;

    /// Rule identifier.
    fn report_rule(&self) -> Rule;

    /// Optional hint for fixing the issue.
    fn hint(&self) -> Option<&str> {
        None
    }

    /// Optional details for the "= note:" line.
    fn details(&self) -> Option<String> {
        None
    }

    /// Usage locations (for replica-lag, untranslated, type-mismatch).
    fn usages(&self) -> &[ResolvedKeyUsage] {
        &[]
    }
}

// ============================================================
// Report Implementations
// ============================================================

impl Report for HardcodedTextIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::Source(&self.context)
    }

    fn message(&self) -> String {
        self.text.clone()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }
}

impl Report for MissingKeyIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::Source(&self.context)
    }

    fn message(&self) -> String {
        self.key.clone()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }

    fn details(&self) -> Option<String> {
        self.from_schema
            .as_ref()
            .map(|(name, file)| format!("from {} ({})", name, file))
    }
}

impl Report for UnresolvedKeyIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::Source(&self.context)
    }

    fn message(&self) -> String {
        self.reason.to_string()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }

    fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
}

impl Report for UnusedKeyIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::Message(&self.context)
    }

    fn message(&self) -> String {
        self.context.key.clone()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }

    fn details(&self) -> Option<String> {
        Some(format!("(\"{}\")", self.context.value))
    }
}

impl Report for OrphanKeyIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::Message(&self.context)
    }

    fn message(&self) -> String {
        self.context.key.clone()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }

    fn details(&self) -> Option<String> {
        Some(format!("in {} (\"{}\")", self.locale, self.context.value))
    }
}

impl Report for ReplicaLagIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::Message(&self.context)
    }

    fn message(&self) -> String {
        self.context.key.clone()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }

    fn details(&self) -> Option<String> {
        Some(format!(
            "(\"{}\") missing in: {}",
            self.context.value,
            self.missing_in.join(", ")
        ))
    }

    fn usages(&self) -> &[ResolvedKeyUsage] {
        &self.usages
    }
}

impl Report for UntranslatedIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::Message(&self.context)
    }

    fn message(&self) -> String {
        self.context.key.clone()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }

    fn details(&self) -> Option<String> {
        Some(format!(
            "(\"{}\") identical in: {}",
            self.context.value,
            self.identical_in.join(", ")
        ))
    }

    fn usages(&self) -> &[ResolvedKeyUsage] {
        &self.usages
    }
}

impl Report for TypeMismatchIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::Message(&self.context)
    }

    fn message(&self) -> String {
        self.context.key.clone()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }

    fn details(&self) -> Option<String> {
        let mismatches: Vec<String> = self
            .mismatched_in
            .iter()
            .map(|m| format!("{} ({})", m.locale, m.actual_type))
            .collect();
        Some(format!(
            "expected {}, got: {}",
            self.expected_type,
            mismatches.join(", ")
        ))
    }

    fn usages(&self) -> &[ResolvedKeyUsage] {
        &self.usages
    }
}

impl Report for ParseErrorIssue {
    fn location(&self) -> ReportLocation<'_> {
        ReportLocation::File {
            path: &self.file_path,
        }
    }

    fn message(&self) -> String {
        self.error.clone()
    }

    fn report_severity(&self) -> Severity {
        Self::severity()
    }

    fn report_rule(&self) -> Rule {
        Self::rule()
    }
}

// ============================================================
// Ordering for Issue (for sorting in reports)
// ============================================================

impl Issue {
    /// Get file path for sorting.
    fn sort_file_path(&self) -> Option<&str> {
        match self.location() {
            ReportLocation::Source(ctx) => Some(&ctx.location.file_path),
            ReportLocation::Message(ctx) => Some(&ctx.location.file_path),
            ReportLocation::File { path } => Some(path),
        }
    }

    /// Get line number for sorting.
    fn sort_line(&self) -> usize {
        match self.location() {
            ReportLocation::Source(ctx) => ctx.location.line,
            ReportLocation::Message(ctx) => ctx.location.line,
            ReportLocation::File { .. } => 0,
        }
    }

    /// Get column number for sorting.
    fn sort_col(&self) -> usize {
        match self.location() {
            ReportLocation::Source(ctx) => ctx.location.col,
            ReportLocation::Message(ctx) => ctx.location.col,
            ReportLocation::File { .. } => 0,
        }
    }
}

impl Ord for Issue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // Sort by: file_path (None last), line, col, message
        match (self.sort_file_path(), other.sort_file_path()) {
            (Some(a), Some(b)) => a
                .cmp(b)
                .then_with(|| self.sort_line().cmp(&other.sort_line()))
                .then_with(|| self.sort_col().cmp(&other.sort_col()))
                .then_with(|| self.message().cmp(&other.message())),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => self
                .rule()
                .cmp(&other.rule())
                .then_with(|| self.message().cmp(&other.message())),
        }
    }
}

impl PartialOrd for Issue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use crate::core::{CommentStyle, MessageLocation, SourceLocation};
    use crate::issues::*;

    #[test]
    fn test_hardcoded_issue() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        let ctx = SourceContext::new(loc, "const x = \"Hello\";", CommentStyle::Js);
        let issue = HardcodedTextIssue {
            context: ctx,
            text: "Hello".to_string(),
        };

        assert_eq!(HardcodedTextIssue::severity(), Severity::Error);
        assert_eq!(HardcodedTextIssue::rule(), Rule::HardcodedText);
        assert_eq!(issue.text, "Hello");
    }

    #[test]
    fn test_missing_key_issue() {
        let loc = SourceLocation::new("./src/app.tsx", 15, 10);
        let ctx = SourceContext::new(loc, "t('Common.missing')", CommentStyle::Js);
        let issue = MissingKeyIssue {
            context: ctx,
            key: "Common.missing".to_string(),
            from_schema: None,
        };

        assert_eq!(MissingKeyIssue::severity(), Severity::Error);
        assert_eq!(issue.key, "Common.missing");
        assert!(issue.from_schema.is_none());
    }

    #[test]
    fn test_missing_key_issue_from_schema() {
        let loc = SourceLocation::new("./src/form.tsx", 20, 5);
        let ctx = SourceContext::new(loc, "formSchema(t)", CommentStyle::Js);
        let issue = MissingKeyIssue {
            context: ctx,
            key: "Form.email".to_string(),
            from_schema: Some((
                "formSchema".to_string(),
                "./src/schemas/form.ts".to_string(),
            )),
        };

        assert!(issue.from_schema.is_some());
        let (name, file) = issue.from_schema.unwrap();
        assert_eq!(name, "formSchema");
        assert_eq!(file, "./src/schemas/form.ts");
    }

    #[test]
    fn test_unresolved_key_issue() {
        let loc = SourceLocation::new("./src/app.tsx", 25, 8);
        let ctx = SourceContext::new(loc, "t(`status.${code}`)", CommentStyle::Jsx);
        let issue = UnresolvedKeyIssue {
            context: ctx,
            reason: IssueUnresolvedKeyReason::TemplateWithExpr,
            hint: Some("Use glot-message-keys annotation".to_string()),
            pattern: Some("status.*".to_string()),
        };

        assert_eq!(UnresolvedKeyIssue::severity(), Severity::Warning);
        assert_eq!(issue.reason, IssueUnresolvedKeyReason::TemplateWithExpr);
        assert_eq!(issue.pattern, Some("status.*".to_string()));
    }

    #[test]
    fn test_unresolved_key_reason_display() {
        assert_eq!(
            IssueUnresolvedKeyReason::VariableKey.to_string(),
            "variable key"
        );
        assert_eq!(
            IssueUnresolvedKeyReason::TemplateWithExpr.to_string(),
            "template with expression"
        );
        assert_eq!(
            IssueUnresolvedKeyReason::UnknownNamespace {
                schema_name: "formSchema".to_string()
            }
            .to_string(),
            "unknown namespace for schema 'formSchema'"
        );
    }

    #[test]
    fn test_unused_key_issue() {
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Common.unused", "Unused Value");
        let issue = UnusedKeyIssue { context: ctx };

        assert_eq!(UnusedKeyIssue::severity(), Severity::Warning);
        assert_eq!(UnusedKeyIssue::rule(), Rule::UnusedKey);
        assert_eq!(issue.context.key, "Common.unused");
    }

    #[test]
    fn test_orphan_key_issue() {
        let loc = MessageLocation::new("./messages/zh.json", 10, 3);
        let ctx = MessageContext::new(loc, "Common.orphan", "orphan value");
        let issue = OrphanKeyIssue {
            context: ctx,
            locale: "zh".to_string(),
        };

        assert_eq!(OrphanKeyIssue::severity(), Severity::Warning);
        assert_eq!(issue.locale, "zh");
    }

    #[test]
    fn test_replica_lag_issue() {
        use crate::core::FullKey;
        use std::collections::HashSet;

        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Common.submit", "Submit");

        let usage_loc = SourceLocation::new("./src/Button.tsx", 25, 10);
        let usage_ctx = SourceContext::new(usage_loc, "{t('Common.submit')}", CommentStyle::Jsx);
        let usage = ResolvedKeyUsage {
            key: FullKey::new("Common.submit"),
            context: usage_ctx,
            suppressed_rules: HashSet::new(),
            from_schema: None,
        };

        let issue = ReplicaLagIssue {
            context: ctx,
            primary_locale: "en".to_string(),
            missing_in: vec!["zh".to_string(), "ja".to_string()],
            usages: vec![usage],
        };

        assert_eq!(ReplicaLagIssue::severity(), Severity::Error);
        assert_eq!(issue.missing_in, vec!["zh", "ja"]);
        assert_eq!(issue.usages.len(), 1);
    }

    #[test]
    fn test_untranslated_issue() {
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Common.ok", "OK");

        let issue = UntranslatedIssue {
            context: ctx,
            primary_locale: "en".to_string(),
            identical_in: vec!["zh".to_string()],
            usages: vec![],
        };

        assert_eq!(UntranslatedIssue::severity(), Severity::Warning);
        assert_eq!(issue.identical_in, vec!["zh"]);
    }

    #[test]
    fn test_type_mismatch_issue() {
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Page.items", "[\"a\", \"b\"]");

        let mismatch_loc = MessageLocation::new("./messages/zh.json", 8, 3);
        let mismatch = LocaleTypeMismatch::new("zh", ValueType::String, mismatch_loc);

        let issue = TypeMismatchIssue {
            context: ctx,
            expected_type: ValueType::StringArray,
            primary_locale: "en".to_string(),
            mismatched_in: vec![mismatch],
            usages: vec![],
        };

        assert_eq!(TypeMismatchIssue::severity(), Severity::Error);
        assert_eq!(issue.expected_type, ValueType::StringArray);
        assert_eq!(issue.mismatched_in.len(), 1);
    }

    #[test]
    fn test_parse_error_issue() {
        let issue = ParseErrorIssue {
            file_path: "./src/broken.tsx".to_string(),
            error: "Unexpected token at line 5".to_string(),
        };

        assert_eq!(ParseErrorIssue::severity(), Severity::Error);
        assert_eq!(ParseErrorIssue::rule(), Rule::ParseError);
        assert_eq!(issue.file_path, "./src/broken.tsx");
    }

    #[test]
    fn test_issue_enum_severity() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        let ctx = SourceContext::new(loc, "const x = \"Hello\";", CommentStyle::Js);
        let issue = Issue::HardcodedText(HardcodedTextIssue {
            context: ctx,
            text: "Hello".to_string(),
        });

        assert_eq!(issue.severity(), Severity::Error);
        assert_eq!(issue.rule(), Rule::HardcodedText);
    }

    #[test]
    fn test_issue_enum_rule() {
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Common.unused", "Unused");
        let issue = Issue::UnusedKey(UnusedKeyIssue { context: ctx });

        assert_eq!(issue.severity(), Severity::Warning);
        assert_eq!(issue.rule(), Rule::UnusedKey);
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(Severity::Error.to_string(), "error");
        assert_eq!(Severity::Warning.to_string(), "warning");
    }

    #[test]
    fn test_rule_display() {
        assert_eq!(Rule::HardcodedText.to_string(), "hardcoded");
        assert_eq!(Rule::MissingKey.to_string(), "missing-key");
        assert_eq!(Rule::UnresolvedKey.to_string(), "unresolved-key");
        assert_eq!(Rule::ReplicaLag.to_string(), "replica-lag");
        assert_eq!(Rule::UnusedKey.to_string(), "unused-key");
        assert_eq!(Rule::OrphanKey.to_string(), "orphan-key");
        assert_eq!(Rule::Untranslated.to_string(), "untranslated");
        assert_eq!(Rule::TypeMismatch.to_string(), "type-mismatch");
        assert_eq!(Rule::ParseError.to_string(), "parse-error");
    }
}

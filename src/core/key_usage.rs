//! Key usage types for Phase 3: Resolution.
//!
//! These types represent the output of the resolve phase (Phase 3) and serve
//! as input to the rules phase (Phase 3+).
//!
//! # Phase Context
//!
//! - **Created in**: Phase 3 (Resolution) from `RawTranslationCall` data
//! - **Consumed in**: Phase 3+ (Rules) to generate user-facing issues

use std::collections::{HashMap, HashSet};

use crate::core::collect::SuppressibleRule;

use crate::core::SourceContext;

// ============================================================
// Unresolved Key Reason
// ============================================================

/// Reason why a key cannot be resolved (internal version for Phase 3).
///
/// This enum is used in `UnresolvedKeyUsage` to track why a translation key
/// couldn't be statically analyzed during Phase 3 resolution.
///
/// **Related**: See `crate::issues::IssueUnresolvedKeyReason` for the user-facing
/// version used in CLI/MCP reporting. The two enums are parallel but serve
/// different purposes:
/// - **This enum** (`UsageUnresolvedKeyReason`): Internal, with extra data fields
/// - **`IssueUnresolvedKeyReason`**: User-facing, simpler structure for reporting
///
/// When converting `UnresolvedKeyUsage` to `UnresolvedKeyIssue`, we map this enum
/// to the user-facing version, dropping internal-only fields like `raw_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageUnresolvedKeyReason {
    /// Key is a variable: `t(keyName)`
    VariableKey,

    /// Key is a template with expressions: `t(\`${prefix}.key\`)`
    TemplateWithExpr,

    /// Namespace cannot be determined for schema-derived keys.
    UnknownNamespace {
        /// Schema function name (e.g., "loginSchema").
        schema_name: String,
        /// The unresolved key pattern (kept for internal tracking).
        raw_key: String,
    },
}

impl std::fmt::Display for UsageUnresolvedKeyReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsageUnresolvedKeyReason::VariableKey => write!(f, "variable key"),
            UsageUnresolvedKeyReason::TemplateWithExpr => write!(f, "template with expression"),
            UsageUnresolvedKeyReason::UnknownNamespace { schema_name, .. } => {
                write!(f, "unknown namespace for schema '{}'", schema_name)
            }
        }
    }
}

// ============================================================
// Key Usage Types
// ============================================================

/// Full translation key (newtype for type safety).
///
/// Represents a complete, namespace-qualified key ready for lookup in locale files.
///
/// # Examples
///
/// - `"Common.submit"` (namespace "Common", key "submit")
/// - `"Home.title"` (namespace "Home", key "title")
/// - `"errors.validation.required"` (namespace "errors", nested keys)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FullKey(pub String);

impl FullKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FullKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Schema source information for keys derived from schema functions.
///
/// When a translation key comes from a schema call like `loginSchema(t)`,
/// we track the schema name and file for better error reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSource {
    /// Schema function name (e.g., "loginSchema").
    pub schema_name: String,
    /// File where the schema is defined (relative to source root).
    pub schema_file: String,
}

/// A resolved translation key usage (Phase 3 output).
///
/// This represents a single `t("key")` or dynamic key call that was
/// successfully resolved to one or more translation keys.
///
/// **Created in**: Phase 3 (Resolution) when a translation call's key can be
/// statically determined and matched to namespaces.
///
/// **Used in**: Phase 3+ (Rules) to check if keys exist in locale files,
/// detect type mismatches, find unused keys, etc.
///
/// # Examples
///
/// ```ignore
/// // Direct call with literal key
/// t("submit") → ResolvedKeyUsage { key: FullKey("Common.submit"), ... }
///
/// // Dynamic call with resolved template
/// KEYS.map(k => t(`prefix.${k}`)) → Multiple ResolvedKeyUsage entries
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeyUsage {
    /// The resolved full key (namespace + key path).
    pub key: FullKey,

    /// Source code context (file, line, column, source line, comment style).
    pub context: SourceContext,

    /// Rules that are suppressed for this usage via glot-disable comments.
    /// Checked by rules to skip reporting suppressed issues.
    pub suppressed_rules: HashSet<SuppressibleRule>,

    /// If this key came from a schema function call (e.g., `loginSchema(t)`),
    /// track the schema source for better error messages.
    pub from_schema: Option<SchemaSource>,
}

/// An unresolved translation key usage (Phase 3 output).
///
/// This represents a `t(...)` call where the key could not be
/// statically resolved (e.g., variable key, complex template).
///
/// **Created in**: Phase 3 (Resolution) when a translation call's key cannot be
/// statically determined (dynamic expression, unknown variable, etc.).
///
/// **Used in**: Phase 3+ (Rules) to generate `UnresolvedKeyIssue` warnings,
/// and by the fix command to suggest `glot-message-keys` comments.
///
/// # Examples
///
/// ```ignore
/// // Variable key
/// t(keyName) → UnresolvedKeyUsage { reason: VariableKey, ... }
///
/// // Complex template
/// t(`${a}.${b}`) → UnresolvedKeyUsage { reason: TemplateWithExpr, ... }
/// ```
#[derive(Debug, Clone)]
pub struct UnresolvedKeyUsage {
    /// Source code context (file, line, column, source line, comment style).
    pub context: SourceContext,

    /// Reason why the key could not be resolved.
    pub reason: UsageUnresolvedKeyReason,

    /// Hint for the user on how to fix (formatted message).
    /// Example: "Consider using a glot-message-keys comment to declare expected keys"
    pub hint: Option<String>,

    /// Pattern inferred from template (e.g., "Common.*.submit").
    /// Used by the fix command to generate `glot-message-keys` comments.
    /// Only present for simple template patterns like `\`prefix.\${var}\``.
    pub pattern: Option<String>,
}

/// Key usages extracted from a single file (Phase 3 output).
///
/// This is the output of `resolve_translation_calls()` for one file.
///
/// **Created in**: Phase 3 (Resolution)
/// **Consumed in**: Phase 3+ (Rules) to check against locale files
#[derive(Debug, Default, Clone)]
pub struct FileKeyUsages {
    /// Successfully resolved key usages (can be checked against locale files).
    pub resolved: Vec<ResolvedKeyUsage>,

    /// Unresolved key usages (generate warnings, cannot be validated).
    pub unresolved: Vec<UnresolvedKeyUsage>,
}

/// All key usages across the codebase, indexed by file path.
///
/// **Phase 3**: Created by resolving all `RawTranslationCall`s
/// **Phase 3+**: Consumed by rules to generate issues
///
/// **Key format**: File path (relative to source root)
pub type AllKeyUsages = HashMap<String, FileKeyUsages>;

// ============================================================
// Hardcoded Text
// ============================================================

/// Hardcoded text found in source code (Phase 2 output).
///
/// Directly collected during Phase 2 (Extraction) and passed through
/// to Phase 3+ (Rules) without modification.
#[derive(Debug, Clone)]
pub struct HardcodedText {
    /// Source code context where the hardcoded text was found.
    pub context: SourceContext,

    /// The hardcoded text content (e.g., "Submit", "Loading...").
    pub text: String,
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use crate::core::key_usage::*;
    use crate::core::{CommentStyle, SourceLocation};

    #[test]
    fn test_full_key() {
        let key = FullKey::new("Common.submit");
        assert_eq!(key.as_str(), "Common.submit");
        assert_eq!(key.to_string(), "Common.submit");
    }

    #[test]
    fn test_unresolved_key_reason_display() {
        assert_eq!(
            UsageUnresolvedKeyReason::VariableKey.to_string(),
            "variable key"
        );
        assert_eq!(
            UsageUnresolvedKeyReason::TemplateWithExpr.to_string(),
            "template with expression"
        );
        assert_eq!(
            UsageUnresolvedKeyReason::UnknownNamespace {
                schema_name: "formSchema".to_string(),
                raw_key: "email".to_string(),
            }
            .to_string(),
            "unknown namespace for schema 'formSchema'"
        );
    }

    #[test]
    fn test_file_key_usages_default() {
        let usages = FileKeyUsages::default();
        assert!(usages.resolved.is_empty());
        assert!(usages.unresolved.is_empty());
    }

    #[test]
    fn test_resolved_key_usage() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        let ctx = SourceContext::new(loc, "t('Common.submit')", CommentStyle::Js);
        let usage = ResolvedKeyUsage {
            key: FullKey::new("Common.submit"),
            context: ctx,
            suppressed_rules: HashSet::new(),
            from_schema: None,
        };
        assert_eq!(usage.key.as_str(), "Common.submit");
        assert!(usage.from_schema.is_none());
    }

    #[test]
    fn test_resolved_key_usage_from_schema() {
        let loc = SourceLocation::new("./src/form.tsx", 20, 5);
        let ctx = SourceContext::new(loc, "formSchema(t)", CommentStyle::Js);
        let usage = ResolvedKeyUsage {
            key: FullKey::new("Form.email"),
            context: ctx,
            suppressed_rules: HashSet::new(),
            from_schema: Some(SchemaSource {
                schema_name: "formSchema".to_string(),
                schema_file: "./src/schemas/form.ts".to_string(),
            }),
        };
        assert!(usage.from_schema.is_some());
        let schema = usage.from_schema.unwrap();
        assert_eq!(schema.schema_name, "formSchema");
    }

    #[test]
    fn test_unresolved_key_usage() {
        let loc = SourceLocation::new("./src/app.tsx", 15, 8);
        let ctx = SourceContext::new(loc, "t(keyVar)", CommentStyle::Jsx);
        let usage = UnresolvedKeyUsage {
            context: ctx,
            reason: UsageUnresolvedKeyReason::VariableKey,
            hint: Some("use glot-message-keys".to_string()),
            pattern: None,
        };
        assert_eq!(usage.reason, UsageUnresolvedKeyReason::VariableKey);
        assert!(usage.hint.is_some());
    }
}

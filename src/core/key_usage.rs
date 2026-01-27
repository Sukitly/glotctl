//! Key usage types for translation key extraction.
//!
//! These types represent the resolved translation key usages from source code.
//! They are the output of the resolve phase and input to the rules phase.

use std::collections::{HashMap, HashSet};

use crate::core::collect::SuppressibleRule;

use crate::core::SourceContext;

// ============================================================
// Unresolved Key Reason
// ============================================================

/// Reason why a key cannot be resolved (statically analyzed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageUnresolvedKeyReason {
    /// Key is a variable: `t(keyName)`
    VariableKey,
    /// Key is a template with expressions: `t(\`${prefix}.key\`)`
    TemplateWithExpr,
    /// Namespace cannot be determined for schema-derived keys.
    UnknownNamespace {
        schema_name: String,
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
/// Example: `"Common.submit"`, `"Home.title"`
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSource {
    pub schema_name: String,
    pub schema_file: String,
}

/// A resolved translation key usage.
///
/// This represents a single `t("key")` or dynamic key call that was
/// successfully resolved to one or more translation keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeyUsage {
    /// The resolved full key.
    pub key: FullKey,
    /// Source code context (location, source_line, comment_style).
    pub context: SourceContext,
    /// Rules that are suppressed for this usage via glot-disable comments.
    pub suppressed_rules: HashSet<SuppressibleRule>,
    /// If this key came from a schema function, the schema source info.
    pub from_schema: Option<SchemaSource>,
}

/// An unresolved translation key usage.
///
/// This represents a `t(...)` call where the key could not be
/// statically resolved (e.g., variable key, complex template).
#[derive(Debug, Clone)]
pub struct UnresolvedKeyUsage {
    /// Source code context (location, source_line, comment_style).
    pub context: SourceContext,
    /// Reason why the key could not be resolved.
    pub reason: UsageUnresolvedKeyReason,
    /// Hint for the user on how to fix (formatted message).
    pub hint: Option<String>,
    /// Pattern inferred from template (e.g., "Common.*.submit").
    /// Used by fix command to generate glot-message-keys comments.
    pub pattern: Option<String>,
}

/// Key usages extracted from a single file.
///
/// This is the output of resolve_translation_calls() for one file.
#[derive(Debug, Default, Clone)]
pub struct FileKeyUsages {
    /// Successfully resolved key usages.
    pub resolved: Vec<ResolvedKeyUsage>,
    /// Unresolved key usages (warnings).
    pub unresolved: Vec<UnresolvedKeyUsage>,
}

/// All key usages indexed by file path.
pub type AllKeyUsages = HashMap<String, FileKeyUsages>;

// ============================================================
// Hardcoded Text
// ============================================================

/// Hardcoded text found in source code.
#[derive(Debug, Clone)]
pub struct HardcodedText {
    /// Source code context where the hardcoded text was found.
    pub context: SourceContext,
    /// The hardcoded text content.
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

//! Unresolved translation key detection rule.
//!
//! Detects translation keys that cannot be statically resolved, including:
//! - Variable keys: `t(keyVar)`
//! - Template literals with expressions: `t(\`prefix.${suffix}\`)`
//! - Unknown namespace for schema-derived keys

use crate::{
    core::CheckContext,
    core::SourceContext,
    core::{AllKeyUsages, UsageUnresolvedKeyReason as ExtractedReason},
    issues::{IssueUnresolvedKeyReason, UnresolvedKeyIssue},
};

pub fn check_unresolved_keys_issues(ctx: &CheckContext) -> Vec<UnresolvedKeyIssue> {
    let key_usages = ctx.all_key_usages();
    check_unresolved_keys(key_usages)
}

/// Check for unresolved translation keys.
///
/// Converts unresolved key usages from the extraction phase into UnresolvedKeyIssue.
/// This includes keys that cannot be statically analyzed due to:
/// - Variable keys
/// - Template literals with expressions
/// - Unknown namespace for schema-derived keys
///
/// # Arguments
/// * `extractions` - All key usages extracted from source files
///
/// # Returns
/// Vector of UnresolvedKeyIssue for keys that cannot be statically resolved
pub fn check_unresolved_keys(extractions: &AllKeyUsages) -> Vec<UnresolvedKeyIssue> {
    let mut issues = Vec::new();

    for file_usages in extractions.values() {
        for unresolved in &file_usages.unresolved {
            let reason = convert_reason(&unresolved.reason);

            issues.push(UnresolvedKeyIssue {
                context: SourceContext::new(
                    unresolved.context.location.clone(),
                    unresolved.context.source_line.clone(),
                    unresolved.context.comment_style,
                ),
                reason,
                hint: unresolved.hint.clone(),
                pattern: unresolved.pattern.clone(),
            });
        }
    }

    issues
}

/// Convert from extraction's UsageUnresolvedKeyReason to issue's IssueUnresolvedKeyReason.
fn convert_reason(reason: &ExtractedReason) -> IssueUnresolvedKeyReason {
    match reason {
        ExtractedReason::VariableKey => IssueUnresolvedKeyReason::VariableKey,
        ExtractedReason::TemplateWithExpr => IssueUnresolvedKeyReason::TemplateWithExpr,
        ExtractedReason::UnknownNamespace { schema_name, .. } => {
            IssueUnresolvedKeyReason::UnknownNamespace {
                schema_name: schema_name.clone(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::{CommentStyle, SourceLocation};
    use crate::core::{FileKeyUsages, UnresolvedKeyUsage};
    use crate::rules::unresolved::*;
    use std::collections::HashMap;

    fn create_unresolved_usage(
        file: &str,
        line: usize,
        reason: ExtractedReason,
    ) -> UnresolvedKeyUsage {
        UnresolvedKeyUsage {
            context: SourceContext::new(
                SourceLocation::new(file, line, 1),
                "t(dynamicKey)",
                CommentStyle::Js,
            ),
            reason,
            hint: None,
            pattern: None,
        }
    }

    #[test]
    fn test_check_unresolved_key_empty() {
        let extractions: AllKeyUsages = HashMap::new();
        let issues = check_unresolved_keys(&extractions);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_unresolved_key_variable() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![],
                unresolved: vec![create_unresolved_usage(
                    "test.tsx",
                    10,
                    ExtractedReason::VariableKey,
                )],
            },
        );

        let issues = check_unresolved_keys(&extractions);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].reason, IssueUnresolvedKeyReason::VariableKey);
    }

    #[test]
    fn test_check_unresolved_key_template() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![],
                unresolved: vec![create_unresolved_usage(
                    "test.tsx",
                    20,
                    ExtractedReason::TemplateWithExpr,
                )],
            },
        );

        let issues = check_unresolved_keys(&extractions);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].reason, IssueUnresolvedKeyReason::TemplateWithExpr);
    }

    #[test]
    fn test_check_unresolved_key_unknown_namespace() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![],
                unresolved: vec![create_unresolved_usage(
                    "test.tsx",
                    30,
                    ExtractedReason::UnknownNamespace {
                        schema_name: "formSchema".to_string(),
                        raw_key: "email".to_string(),
                    },
                )],
            },
        );

        let issues = check_unresolved_keys(&extractions);
        assert_eq!(issues.len(), 1);
        match &issues[0].reason {
            IssueUnresolvedKeyReason::UnknownNamespace { schema_name } => {
                assert_eq!(schema_name, "formSchema");
            }
            _ => panic!("Expected UnknownNamespace reason"),
        }
    }

    #[test]
    fn test_check_unresolved_key_with_hint_and_pattern() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![],
                unresolved: vec![UnresolvedKeyUsage {
                    context: SourceContext::new(
                        SourceLocation::new("test.tsx", 10, 5),
                        "t(`status.${code}`)",
                        CommentStyle::Jsx,
                    ),
                    reason: ExtractedReason::TemplateWithExpr,
                    hint: Some("Use glot-message-keys annotation".to_string()),
                    pattern: Some("status.*".to_string()),
                }],
            },
        );

        let issues = check_unresolved_keys(&extractions);
        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues[0].hint,
            Some("Use glot-message-keys annotation".to_string())
        );
        assert_eq!(issues[0].pattern, Some("status.*".to_string()));
    }

    #[test]
    fn test_check_unresolved_key_multiple_files() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "a.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![],
                unresolved: vec![create_unresolved_usage(
                    "a.tsx",
                    1,
                    ExtractedReason::VariableKey,
                )],
            },
        );
        extractions.insert(
            "b.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![],
                unresolved: vec![create_unresolved_usage(
                    "b.tsx",
                    2,
                    ExtractedReason::TemplateWithExpr,
                )],
            },
        );

        let issues = check_unresolved_keys(&extractions);
        assert_eq!(issues.len(), 2);
    }
}

//! InsertMessageKeys action.
//!
//! Inserts `glot-message-keys` comments for dynamic translation keys.
//! Used by the `glot fix` command.

use crate::core::CommentStyle;
use crate::issues::UnresolvedKeyIssue;

use crate::actions::operation::Operation;
use crate::actions::traits::{Action, ActionStats};

/// Action to insert `glot-message-keys` comments.
///
/// This action processes `UnresolvedKeyIssue` that have a pattern field,
/// inserting a comment that declares the expected translation keys.
pub struct InsertMessageKeys;

impl InsertMessageKeys {
    /// Format a message-keys comment for the given pattern.
    fn format_comment(pattern: &str, comment_style: CommentStyle) -> String {
        let directive = format!("glot-message-keys \"{}\"", pattern);
        match comment_style {
            CommentStyle::Js => format!("// {}", directive),
            CommentStyle::Jsx => format!("{{/* {} */}}", directive),
        }
    }
}

impl Action<UnresolvedKeyIssue> for InsertMessageKeys {
    fn to_operations(issues: &[UnresolvedKeyIssue]) -> Vec<Operation> {
        issues
            .iter()
            .filter_map(|issue| {
                issue
                    .pattern
                    .as_ref()
                    .map(|pattern| Operation::InsertComment {
                        context: issue.context.clone(),
                        comment: Self::format_comment(pattern, issue.context.comment_style),
                    })
            })
            .collect()
    }

    /// Override run to track skipped issues (those without pattern).
    fn run(issues: &[UnresolvedKeyIssue]) -> anyhow::Result<ActionStats> {
        let ops = Self::to_operations(issues);
        let processed = ops.len();
        let skipped = issues.len() - processed; // Issues without pattern

        let mut files_modified = std::collections::HashSet::new();
        for op in &ops {
            op.execute()?;
            if let Operation::InsertComment { context, .. } = op {
                files_modified.insert(context.file_path().to_string());
            }
        }

        Ok(ActionStats {
            processed,
            skipped,
            files_modified: files_modified.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::actions::insert_message_keys::*;
    use crate::core::{SourceContext, SourceLocation};
    use crate::issues::IssueUnresolvedKeyReason;

    #[test]
    fn test_format_comment_js() {
        let comment = InsertMessageKeys::format_comment("Common.status.*", CommentStyle::Js);
        assert_eq!(comment, "// glot-message-keys \"Common.status.*\"");
    }

    #[test]
    fn test_format_comment_jsx() {
        let comment = InsertMessageKeys::format_comment("Common.status.*", CommentStyle::Jsx);
        assert_eq!(comment, "{/* glot-message-keys \"Common.status.*\" */}");
    }

    #[test]
    fn test_to_operations_with_pattern() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        let ctx = SourceContext::new(loc, "t(`status.${code}`)", CommentStyle::Jsx);
        let issue = UnresolvedKeyIssue {
            context: ctx,
            reason: IssueUnresolvedKeyReason::TemplateWithExpr,
            hint: Some("Use glot-message-keys".to_string()),
            pattern: Some("status.*".to_string()),
        };

        let ops = InsertMessageKeys::to_operations(&[issue]);

        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Operation::InsertComment { context, comment } => {
                assert_eq!(context.file_path(), "./src/app.tsx");
                assert_eq!(comment, "{/* glot-message-keys \"status.*\" */}");
            }
            _ => panic!("Expected InsertComment"),
        }
    }

    #[test]
    fn test_to_operations_without_pattern() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        let ctx = SourceContext::new(loc, "t(keyVar)", CommentStyle::Js);
        let issue = UnresolvedKeyIssue {
            context: ctx,
            reason: IssueUnresolvedKeyReason::VariableKey,
            hint: None,
            pattern: None, // No pattern - cannot be fixed
        };

        let ops = InsertMessageKeys::to_operations(&[issue]);

        assert_eq!(ops.len(), 0); // Skipped
    }

    #[test]
    fn test_to_operations_mixed() {
        let loc1 = SourceLocation::new("./src/a.tsx", 10, 5);
        let ctx1 = SourceContext::new(loc1, "t(`status.${code}`)", CommentStyle::Jsx);
        let issue1 = UnresolvedKeyIssue {
            context: ctx1,
            reason: IssueUnresolvedKeyReason::TemplateWithExpr,
            hint: None,
            pattern: Some("status.*".to_string()),
        };

        let loc2 = SourceLocation::new("./src/b.tsx", 20, 3);
        let ctx2 = SourceContext::new(loc2, "t(keyVar)", CommentStyle::Js);
        let issue2 = UnresolvedKeyIssue {
            context: ctx2,
            reason: IssueUnresolvedKeyReason::VariableKey,
            hint: None,
            pattern: None, // No pattern
        };

        let ops = InsertMessageKeys::to_operations(&[issue1, issue2]);

        assert_eq!(ops.len(), 1); // Only one has pattern
    }
}

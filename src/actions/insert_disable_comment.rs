//! InsertDisableComment action.
//!
//! Inserts `glot-disable-next-line` comments to suppress warnings.
//! Used by the `glot baseline` command.

use crate::types::{CommentStyle, HardcodedIssue, Rule, UntranslatedIssue};

use super::operation::Operation;
use super::traits::Action;

/// Action to insert `glot-disable-next-line` comments.
///
/// This action supports multiple Issue types:
/// - `HardcodedIssue`: inserts comment at the issue's context location
/// - `UntranslatedIssue`: inserts comment at each usage location
pub struct InsertDisableComment;

impl InsertDisableComment {
    /// Format a disable comment for the given rule.
    fn format_comment(rule: Rule, comment_style: CommentStyle) -> String {
        let directive = format!("glot-disable-next-line {}", rule);
        match comment_style {
            CommentStyle::Js => format!("// {}", directive),
            CommentStyle::Jsx => format!("{{/* {} */}}", directive),
        }
    }
}

impl Action<HardcodedIssue> for InsertDisableComment {
    fn to_operations(issues: &[HardcodedIssue]) -> Vec<Operation> {
        issues
            .iter()
            .map(|issue| Operation::InsertComment {
                context: issue.context.clone(),
                comment: Self::format_comment(Rule::HardcodedText, issue.context.comment_style),
            })
            .collect()
    }
}

impl Action<UntranslatedIssue> for InsertDisableComment {
    fn to_operations(issues: &[UntranslatedIssue]) -> Vec<Operation> {
        issues
            .iter()
            .flat_map(|issue| {
                issue.usages.iter().map(|ctx| Operation::InsertComment {
                    context: ctx.clone(),
                    comment: Self::format_comment(Rule::Untranslated, ctx.comment_style),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MessageContext, MessageLocation, SourceContext, SourceLocation};

    #[test]
    fn test_format_comment_js() {
        let comment = InsertDisableComment::format_comment(Rule::HardcodedText, CommentStyle::Js);
        assert_eq!(comment, "// glot-disable-next-line hardcoded");
    }

    #[test]
    fn test_format_comment_jsx() {
        let comment = InsertDisableComment::format_comment(Rule::Untranslated, CommentStyle::Jsx);
        assert_eq!(comment, "{/* glot-disable-next-line untranslated */}");
    }

    #[test]
    fn test_hardcoded_to_operations() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        let ctx = SourceContext::new(loc, "const x = \"Hello\";", CommentStyle::Js);
        let issue = HardcodedIssue {
            context: ctx,
            text: "Hello".to_string(),
        };

        let ops = InsertDisableComment::to_operations(&[issue]);

        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Operation::InsertComment { context, comment } => {
                assert_eq!(context.file_path(), "./src/app.tsx");
                assert_eq!(comment, "// glot-disable-next-line hardcoded");
            }
            _ => panic!("Expected InsertComment"),
        }
    }

    #[test]
    fn test_untranslated_to_operations() {
        let msg_loc = MessageLocation::new("./messages/en.json", 5, 3);
        let msg_ctx = MessageContext::new(msg_loc, "Common.ok", "OK");

        let usage1_loc = SourceLocation::new("./src/a.tsx", 10, 5);
        let usage1_ctx = SourceContext::new(usage1_loc, "{t('Common.ok')}", CommentStyle::Jsx);

        let usage2_loc = SourceLocation::new("./src/b.tsx", 20, 3);
        let usage2_ctx = SourceContext::new(usage2_loc, "t('Common.ok')", CommentStyle::Js);

        let issue = UntranslatedIssue {
            context: msg_ctx,
            primary_locale: "en".to_string(),
            identical_in: vec!["zh".to_string()],
            usages: vec![usage1_ctx, usage2_ctx],
        };

        let ops = InsertDisableComment::to_operations(&[issue]);

        assert_eq!(ops.len(), 2);

        // First usage - JSX style
        match &ops[0] {
            Operation::InsertComment { context, comment } => {
                assert_eq!(context.file_path(), "./src/a.tsx");
                assert_eq!(comment, "{/* glot-disable-next-line untranslated */}");
            }
            _ => panic!("Expected InsertComment"),
        }

        // Second usage - JS style
        match &ops[1] {
            Operation::InsertComment { context, comment } => {
                assert_eq!(context.file_path(), "./src/b.tsx");
                assert_eq!(comment, "// glot-disable-next-line untranslated");
            }
            _ => panic!("Expected InsertComment"),
        }
    }

    #[test]
    fn test_untranslated_no_usages() {
        let msg_loc = MessageLocation::new("./messages/en.json", 5, 3);
        let msg_ctx = MessageContext::new(msg_loc, "Common.ok", "OK");

        let issue = UntranslatedIssue {
            context: msg_ctx,
            primary_locale: "en".to_string(),
            identical_in: vec!["zh".to_string()],
            usages: vec![], // No usages
        };

        let ops = InsertDisableComment::to_operations(&[issue]);

        assert_eq!(ops.len(), 0);
    }
}

//! Hardcoded text detection rule.
//!
//! Detects untranslated text in JSX/TSX files that should use i18n translation functions.

use crate::{commands::context::AllHardcodedIssues, types::issue::HardcodedIssue};

/// Check for hardcoded text issues.
///
/// Converts raw hardcoded issues from the extraction phase into Issue types.
/// The actual detection is done during the file analysis phase.
///
/// # Arguments
/// * `hardcoded_issues` - All hardcoded text issues from the extraction phase
///
/// # Returns
/// Vector of HardcodedIssue for reporting
pub fn check_hardcoded(hardcoded_issues: &AllHardcodedIssues) -> Vec<HardcodedIssue> {
    let mut issues = Vec::new();

    for file_issues in hardcoded_issues.values() {
        for issue in file_issues {
            // Convert from old issue type to new issue type
            issues.push(HardcodedIssue {
                context: crate::types::context::SourceContext::new(
                    crate::types::context::SourceLocation::new(
                        &issue.location.file_path,
                        issue.location.line,
                        issue.location.col.unwrap_or(1),
                    ),
                    issue.source_line.clone().unwrap_or_default(),
                    if issue.location.in_jsx_context {
                        crate::types::context::CommentStyle::Jsx
                    } else {
                        crate::types::context::CommentStyle::Js
                    },
                ),
                text: issue.text.clone(),
            });
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue::{HardcodedIssue as OldHardcodedIssue, SourceLocation as OldSourceLocation};
    use std::collections::HashMap;

    fn create_old_hardcoded_issue(
        file: &str,
        line: usize,
        col: usize,
        text: &str,
        in_jsx: bool,
    ) -> OldHardcodedIssue {
        OldHardcodedIssue {
            location: OldSourceLocation::new(file, line)
                .with_col(col)
                .with_jsx_context(in_jsx),
            text: text.to_string(),
            source_line: Some(format!("// {}", text)),
        }
    }

    #[test]
    fn test_check_hardcoded_empty() {
        let hardcoded_issues: AllHardcodedIssues = HashMap::new();
        let issues = check_hardcoded(&hardcoded_issues);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_hardcoded_single_file() {
        let mut hardcoded_issues: AllHardcodedIssues = HashMap::new();
        hardcoded_issues.insert(
            "test.tsx".to_string(),
            vec![
                create_old_hardcoded_issue("test.tsx", 10, 5, "Hello", false),
                create_old_hardcoded_issue("test.tsx", 20, 3, "World", true),
            ],
        );

        let issues = check_hardcoded(&hardcoded_issues);
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_check_hardcoded_multiple_files() {
        let mut hardcoded_issues: AllHardcodedIssues = HashMap::new();
        hardcoded_issues.insert(
            "a.tsx".to_string(),
            vec![create_old_hardcoded_issue("a.tsx", 1, 1, "Text A", false)],
        );
        hardcoded_issues.insert(
            "b.tsx".to_string(),
            vec![create_old_hardcoded_issue("b.tsx", 2, 2, "Text B", true)],
        );

        let issues = check_hardcoded(&hardcoded_issues);
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_check_hardcoded_preserves_context() {
        let mut hardcoded_issues: AllHardcodedIssues = HashMap::new();
        hardcoded_issues.insert(
            "test.tsx".to_string(),
            vec![create_old_hardcoded_issue("test.tsx", 10, 5, "Hello", true)],
        );

        let issues = check_hardcoded(&hardcoded_issues);
        assert_eq!(issues.len(), 1);

        let issue = &issues[0];
        assert_eq!(issue.context.location.file_path, "test.tsx");
        assert_eq!(issue.context.location.line, 10);
        assert_eq!(issue.context.location.col, 5);
        assert_eq!(issue.text, "Hello");
        assert!(issue.context.comment_style.is_jsx());
    }
}

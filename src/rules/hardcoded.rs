//! Hardcoded text detection rule.
//!
//! Detects untranslated text in JSX/TSX files that should use i18n translation functions.

use anyhow::Result;

use crate::{
    commands::{check::check_hardcoded, context::CheckContext},
    issue::{Issue, ParseErrorIssue},
    rules::Checker,
};

pub struct HardcodedRule;

impl Checker for HardcodedRule {
    fn name(&self) -> &str {
        "hardcoded"
    }

    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>> {
        let mut issues = Vec::new();

        for file_path in &ctx.files {
            match check_hardcoded(file_path, &ctx.config.checked_attributes, &ctx.ignore_texts) {
                Ok(hardcoded_issues) => {
                    for issue in hardcoded_issues {
                        issues.push(Issue::Hardcoded(issue));
                    }
                }
                Err(e) => {
                    if ctx.verbose {
                        eprintln!("Warning: {} - {}", file_path, e);
                    }
                    issues.push(Issue::ParseError(ParseErrorIssue {
                        file_path: file_path.to_string(),
                        error: format!("Failed to parse: {}", e),
                    }));
                }
            }
        }

        Ok(issues)
    }
}

//! Hardcoded text detection rule.
//!
//! Detects untranslated text in JSX/TSX files that should use i18n translation functions.

use anyhow::Result;

use crate::{
    commands::{check::check_hardcoded, context::CheckContext},
    issue::Issue,
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
                        issues.push(Issue::hardcoded(
                            &issue.file_path,
                            issue.line,
                            issue.col,
                            &issue.text,
                            Some(issue.source_line),
                        ));
                    }
                }
                Err(e) => {
                    if ctx.verbose {
                        eprintln!("Warning: {} - {}", file_path, e);
                    }
                    issues.push(Issue::parse_error(file_path, &e.to_string()));
                }
            }
        }

        Ok(issues)
    }
}

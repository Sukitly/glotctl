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
            // Skip files that failed to parse (already recorded in ensure_parsed_files)
            let Some(parsed) = ctx.get_parsed(file_path) else {
                continue;
            };

            let hardcoded_issues = check_hardcoded(
                file_path,
                parsed,
                &ctx.config.checked_attributes,
                &ctx.ignore_texts,
            );
            for issue in hardcoded_issues {
                issues.push(Issue::Hardcoded(issue));
            }
        }

        Ok(issues)
    }
}

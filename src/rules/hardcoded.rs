//! Hardcoded text detection rule.
//!
//! Detects untranslated text in JSX/TSX files that should use i18n translation functions.

use anyhow::Result;

use crate::{commands::context::CheckContext, issue::Issue, rules::Checker};

pub struct HardcodedRule;

impl Checker for HardcodedRule {
    fn name(&self) -> &str {
        "hardcoded"
    }

    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>> {
        // Ensure hardcoded issues are analyzed (uses unified FileAnalyzer)
        ctx.ensure_hardcoded_issues()?;

        let all_issues = ctx
            .hardcoded_issues()
            .expect("hardcoded_issues must be loaded after ensure_hardcoded_issues");

        let mut issues = Vec::new();
        for file_issues in all_issues.values() {
            for issue in file_issues {
                issues.push(Issue::Hardcoded(issue.clone()));
            }
        }

        Ok(issues)
    }
}

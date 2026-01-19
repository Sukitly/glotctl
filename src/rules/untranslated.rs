//! Untranslated value detection rule.
//!
//! Detects translation values that are identical to the primary locale,
//! which may indicate that the text was not translated.

use anyhow::Result;

use crate::{
    commands::context::CheckContext, issue::Issue, rules::Checker, utils::contains_alphabetic,
};

pub struct UntranslatedRule;

impl Checker for UntranslatedRule {
    fn name(&self) -> &str {
        "untranslated"
    }

    fn needs_messages(&self) -> bool {
        true
    }

    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>> {
        ctx.ensure_messages()?;

        let messages = ctx.messages().expect("messages must be loaded");
        let primary_locale = &ctx.config.primary_locale;

        let Some(primary_messages) = &messages.primary_messages else {
            // No primary locale found, skip check (runner will report error)
            return Ok(Vec::new());
        };

        let mut issues = Vec::new();

        // Check each non-primary locale
        for (locale, locale_messages) in &messages.all_messages {
            // Skip primary locale
            if locale == primary_locale {
                continue;
            }

            for (key, entry) in locale_messages {
                // Skip if value has no alphabetic characters (pure numbers/symbols)
                if !contains_alphabetic(&entry.value) {
                    continue;
                }

                // Check if value is identical to primary locale
                if let Some(primary_entry) = primary_messages.get(key)
                    && entry.value == primary_entry.value
                {
                    issues.push(Issue::untranslated(
                        &entry.file_path,
                        entry.line,
                        key,
                        &entry.value,
                        primary_locale,
                    ));
                }
            }
        }

        Ok(issues)
    }
}

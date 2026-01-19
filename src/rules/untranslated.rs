//! Untranslated value detection rule.
//!
//! Detects translation values that are identical to the primary locale,
//! which may indicate that the text was not translated.
//!
//! Output format is consistent with replica-lag:
//! - Points to primary locale file (source of truth)
//! - Shows which locales have identical values
//! - Shows where the key is used in code

use anyhow::Result;

use crate::{
    commands::{
        check::{build_key_disable_map, build_key_usage_map, get_usages_for_key},
        context::CheckContext,
    },
    issue::{Issue, MAX_KEY_USAGES, MessageLocation, UntranslatedIssue},
    rules::Checker,
    utils::contains_alphabetic,
};

pub struct UntranslatedRule;

impl Checker for UntranslatedRule {
    fn name(&self) -> &str {
        "untranslated"
    }

    fn needs_registries(&self) -> bool {
        // Need registries to build extractions for key usages
        true
    }

    fn needs_messages(&self) -> bool {
        true
    }

    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>> {
        ctx.ensure_messages()?;
        // Need extractions to get key usage locations
        ctx.ensure_extractions()?;

        let messages = ctx.messages().expect("messages must be loaded");
        let extractions = ctx.extractions().expect("extractions must be loaded");
        let primary_locale = &ctx.config.primary_locale;

        let Some(primary_messages) = &messages.primary_messages else {
            // No primary locale found, skip check (runner will report error)
            return Ok(Vec::new());
        };

        // Build key usage map for showing where keys are used
        let key_usages = build_key_usage_map(extractions);
        // Build key disable map for checking if untranslated rule is disabled
        let key_disable = build_key_disable_map(extractions);

        let mut issues = Vec::new();

        // Iterate over primary locale keys (like replica-lag)
        for (key, primary_entry) in primary_messages {
            // Skip if value has no alphabetic characters (pure numbers/symbols)
            if !contains_alphabetic(&primary_entry.value) {
                continue;
            }

            // Skip if all usages have untranslated rule disabled
            // (one-vote-veto: if ANY usage is not disabled, report the issue)
            if let Some(stats) = key_disable.get(key)
                && stats.all_disabled()
            {
                continue;
            }

            // Collect locales with identical values
            let mut identical_in: Vec<String> = messages
                .all_messages
                .iter()
                .filter(|(locale, msgs)| {
                    *locale != primary_locale
                        && msgs.get(key).map(|e| &e.value) == Some(&primary_entry.value)
                })
                .map(|(locale, _)| locale.clone())
                .collect();
            identical_in.sort();

            if !identical_in.is_empty() {
                let (usages, total_usages) = get_usages_for_key(&key_usages, key, MAX_KEY_USAGES);
                issues.push(Issue::Untranslated(UntranslatedIssue {
                    location: MessageLocation::new(&primary_entry.file_path, primary_entry.line),
                    key: key.clone(),
                    value: primary_entry.value.clone(),
                    primary_locale: primary_locale.clone(),
                    identical_in,
                    usages,
                    total_usages,
                }));
            }
        }

        issues.sort();
        Ok(issues)
    }
}

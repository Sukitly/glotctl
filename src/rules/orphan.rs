//! Orphan keys detection rule.
//!
//! Detects unused translation keys in locale files, including:
//! - Keys in primary locale that are not used in code
//! - Keys in non-primary locales that don't exist in primary locale

use anyhow::Result;

use crate::{
    commands::{
        check::{find_orphan_keys, find_unused_keys},
        context::CheckContext,
    },
    issue::Issue,
    rules::Checker,
};

pub struct OrphanKeysRule;

impl Checker for OrphanKeysRule {
    fn name(&self) -> &str {
        "orphan_keys"
    }

    fn needs_registries(&self) -> bool {
        true
    }

    fn needs_messages(&self) -> bool {
        true
    }

    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>> {
        // Ensure used_keys are collected
        ctx.ensure_used_keys()?;
        let mut issues = Vec::new();

        let messages = ctx.messages().expect("messages must be loaded");
        let used_keys = ctx.used_keys().expect("used_keys must be loaded");

        if let Some(primary_messages) = &messages.primary_messages {
            issues.extend(find_unused_keys(used_keys, primary_messages));
        }

        issues.extend(find_orphan_keys(
            &ctx.config.primary_locale,
            &messages.all_messages,
        ));

        Ok(issues)
    }
}

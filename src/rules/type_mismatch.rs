//! Type mismatch detection rule.
//!
//! Detects translation values with different types between primary and replica locales.
//! For example: primary has array, but replica has string.
//!
//! This is a critical error because type mismatches cause runtime crashes when the
//! application expects one type (e.g., array for iteration) but gets another (e.g., string).
//!
//! Output format is consistent with untranslated/replica-lag:
//! - Points to primary locale file (source of truth)
//! - Shows which locales have type mismatches with their file locations
//! - Shows where the key is used in code

use anyhow::Result;

use crate::{
    commands::{check::build_key_usage_map, context::CheckContext},
    issue::{Issue, LocaleTypeMismatch, MessageLocation, TypeMismatchIssue},
    rules::Checker,
};

pub struct TypeMismatchRule;

impl Checker for TypeMismatchRule {
    fn name(&self) -> &str {
        "type_mismatch"
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

        let mut issues = Vec::new();

        // Iterate over primary locale keys
        for (key, primary_entry) in primary_messages {
            // Collect locales with type mismatch
            let mut mismatched_in: Vec<LocaleTypeMismatch> = messages
                .all_messages
                .iter()
                .filter_map(|(locale, msgs)| {
                    if locale == primary_locale {
                        return None;
                    }
                    msgs.get(key).and_then(|entry| {
                        if entry.value_type != primary_entry.value_type {
                            Some(LocaleTypeMismatch {
                                locale: locale.clone(),
                                actual_type: entry.value_type,
                                file_path: entry.file_path.clone(),
                                line: entry.line,
                            })
                        } else {
                            None
                        }
                    })
                })
                .collect();
            mismatched_in.sort();

            if !mismatched_in.is_empty() {
                // Store all usages (no limit) so we can show where the key is used
                let usages = key_usages.get(key).cloned().unwrap_or_default();
                let total_usages = usages.len();

                issues.push(Issue::TypeMismatch(TypeMismatchIssue {
                    location: MessageLocation::new(&primary_entry.file_path, primary_entry.line),
                    key: key.clone(),
                    expected_type: primary_entry.value_type,
                    primary_locale: primary_locale.clone(),
                    mismatched_in,
                    usages,
                    total_usages,
                }));
            }
        }

        issues.sort();
        Ok(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_mismatch_rule_name() {
        let rule = TypeMismatchRule;
        assert_eq!(rule.name(), "type_mismatch");
    }

    #[test]
    fn test_type_mismatch_rule_needs() {
        let rule = TypeMismatchRule;
        assert!(rule.needs_registries());
        assert!(rule.needs_messages());
    }
}

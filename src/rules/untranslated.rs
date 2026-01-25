//! Untranslated value detection rule.
//!
//! Detects translation values that are identical to the primary locale,
//! which may indicate that the text was not translated.
//!
//! Output format is consistent with replica-lag:
//! - Points to primary locale file (source of truth)
//! - Shows which locales have identical values
//! - Shows where the key is used in code

use std::collections::HashMap;

use anyhow::Result;

use crate::{
    commands::context::CheckContext,
    issue::Issue,
    parsers::json::MessageMap,
    rules::helpers::{KeyDisableMap, KeyUsageMap, MAX_KEY_USAGES, get_usages_for_key},
    types::{
        context::{MessageContext, MessageLocation},
        issue::UntranslatedIssue,
    },
    utils::contains_alphabetic,
};

// ============================================================
// DEPRECATED: Legacy Checker trait implementation
// TODO: Remove after migrating baseline.rs
// ============================================================

/// Untranslated rule using the Checker trait.
///
/// DEPRECATED: Use `check_untranslated()` function directly instead.
#[allow(deprecated)]
pub struct UntranslatedRule;

#[allow(deprecated)]
impl crate::rules::Checker for UntranslatedRule {
    fn name(&self) -> &str {
        "untranslated"
    }

    fn needs_registries(&self) -> bool {
        true
    }

    fn needs_messages(&self) -> bool {
        true
    }

    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>> {
        ctx.ensure_messages()?;
        ctx.ensure_extractions()?;

        let messages = ctx.messages().expect("messages must be loaded");
        let extractions = ctx.extractions().expect("extractions must be loaded");
        let primary_locale = &ctx.config.primary_locale;

        let Some(primary_messages) = &messages.primary_messages else {
            return Ok(Vec::new());
        };

        let key_usages = crate::rules::build_key_usage_map(extractions);
        let key_disable = crate::rules::build_key_disable_map(extractions);

        let new_issues = check_untranslated(
            primary_locale,
            primary_messages,
            &messages.all_messages,
            &key_usages,
            &key_disable,
        );

        // Convert new issues to old format
        let issues = new_issues
            .into_iter()
            .map(|i| {
                let usages = i
                    .usages
                    .iter()
                    .map(|u| {
                        crate::issue::KeyUsage::new(
                            crate::issue::SourceLocation::new(
                                &u.location.file_path,
                                u.location.line,
                            )
                            .with_col(u.location.col)
                            .with_jsx_context(u.comment_style.is_jsx()),
                        )
                    })
                    .collect::<Vec<_>>();
                let total_usages = usages.len();
                Issue::Untranslated(crate::issue::UntranslatedIssue {
                    location: crate::issue::MessageLocation::new(
                        &i.context.location.file_path,
                        i.context.location.line,
                    ),
                    key: i.context.key,
                    value: i.context.value,
                    primary_locale: i.primary_locale,
                    identical_in: i.identical_in,
                    usages,
                    total_usages,
                })
            })
            .collect();

        Ok(issues)
    }
}

// ============================================================
// Pure function implementation (new API)
// ============================================================

/// Check for untranslated values.
///
/// Finds all keys where the value in non-primary locales is identical to
/// the primary locale value, which may indicate the text was not translated.
///
/// # Arguments
/// * `primary_locale` - The primary locale code (e.g., "en")
/// * `primary_messages` - Messages from the primary locale
/// * `all_messages` - All messages from all locales
/// * `key_usages` - Map of key to usage locations (for showing where keys are used)
/// * `key_disable_map` - Map of key to disable stats (for checking if rule is suppressed)
///
/// # Returns
/// Vector of UntranslatedIssue for keys with identical values across locales
pub fn check_untranslated(
    primary_locale: &str,
    primary_messages: &MessageMap,
    all_messages: &HashMap<String, MessageMap>,
    key_usages: &KeyUsageMap,
    key_disable_map: &KeyDisableMap,
) -> Vec<UntranslatedIssue> {
    let mut issues = Vec::new();

    for (key, primary_entry) in primary_messages {
        // Skip if value has no alphabetic characters (pure numbers/symbols)
        if !contains_alphabetic(&primary_entry.value) {
            continue;
        }

        // Skip if all usages have untranslated rule disabled
        // (one-vote-veto: if ANY usage is not disabled, report the issue)
        if let Some(stats) = key_disable_map.get(key)
            && stats.all_disabled()
        {
            continue;
        }

        // Collect locales with identical values
        let mut identical_in: Vec<String> = all_messages
            .iter()
            .filter(|(locale, msgs)| {
                *locale != primary_locale
                    && msgs.get(key).map(|e| &e.value) == Some(&primary_entry.value)
            })
            .map(|(locale, _)| locale.clone())
            .collect();
        identical_in.sort();

        if !identical_in.is_empty() {
            let (usages, _total) = get_usages_for_key(key_usages, key, MAX_KEY_USAGES);

            issues.push(UntranslatedIssue {
                context: MessageContext::new(
                    MessageLocation::new(&primary_entry.file_path, primary_entry.line, 1),
                    key.clone(),
                    primary_entry.value.clone(),
                ),
                primary_locale: primary_locale.to_string(),
                identical_in,
                usages,
            });
        }
    }

    // Sort by file path, then line for deterministic output
    issues.sort_by(|a, b| {
        a.context
            .location
            .file_path
            .cmp(&b.context.location.file_path)
            .then_with(|| a.context.location.line.cmp(&b.context.location.line))
            .then_with(|| a.context.key.cmp(&b.context.key))
    });

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::json::{MessageEntry, ValueType};

    fn create_message_map(file: &str, entries: &[(&str, &str)]) -> MessageMap {
        entries
            .iter()
            .enumerate()
            .map(|(i, (k, v))| {
                (
                    k.to_string(),
                    MessageEntry {
                        value: v.to_string(),
                        value_type: ValueType::String,
                        file_path: file.to_string(),
                        line: i + 1,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn test_check_untranslated_none() {
        let primary_messages = create_message_map("en.json", &[("Common.submit", "Submit")]);
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.submit", "提交")]),
        );

        let key_usages = KeyUsageMap::new();
        let key_disable_map = KeyDisableMap::new();

        let issues = check_untranslated(
            "en",
            &primary_messages,
            &all_messages,
            &key_usages,
            &key_disable_map,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_untranslated_one_identical() {
        let primary_messages = create_message_map("en.json", &[("Common.ok", "OK")]);
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.ok", "OK")]), // Same as primary
        );

        let key_usages = KeyUsageMap::new();
        let key_disable_map = KeyDisableMap::new();

        let issues = check_untranslated(
            "en",
            &primary_messages,
            &all_messages,
            &key_usages,
            &key_disable_map,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].context.key, "Common.ok");
        assert_eq!(issues[0].identical_in, vec!["zh"]);
    }

    #[test]
    fn test_check_untranslated_multiple_locales() {
        let primary_messages = create_message_map("en.json", &[("Common.ok", "OK")]);
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.ok", "OK")]),
        );
        all_messages.insert(
            "ja".to_string(),
            create_message_map("ja.json", &[("Common.ok", "OK")]),
        );

        let key_usages = KeyUsageMap::new();
        let key_disable_map = KeyDisableMap::new();

        let issues = check_untranslated(
            "en",
            &primary_messages,
            &all_messages,
            &key_usages,
            &key_disable_map,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].identical_in, vec!["ja", "zh"]); // Sorted
    }

    #[test]
    fn test_check_untranslated_skips_numbers() {
        let primary_messages = create_message_map("en.json", &[("Common.number", "123")]);
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.number", "123")]),
        );

        let key_usages = KeyUsageMap::new();
        let key_disable_map = KeyDisableMap::new();

        let issues = check_untranslated(
            "en",
            &primary_messages,
            &all_messages,
            &key_usages,
            &key_disable_map,
        );
        // Should skip because value has no alphabetic characters
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_untranslated_skips_disabled() {
        use crate::rules::helpers::KeyDisableStats;

        let primary_messages = create_message_map("en.json", &[("Common.ok", "OK")]);
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.ok", "OK")]),
        );

        let key_usages = KeyUsageMap::new();
        let mut key_disable_map = KeyDisableMap::new();
        key_disable_map.insert(
            "Common.ok".to_string(),
            KeyDisableStats {
                total_usages: 2,
                disabled_usages: 2, // All disabled
            },
        );

        let issues = check_untranslated(
            "en",
            &primary_messages,
            &all_messages,
            &key_usages,
            &key_disable_map,
        );
        // Should skip because all usages are disabled
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_untranslated_reports_if_some_not_disabled() {
        use crate::rules::helpers::KeyDisableStats;

        let primary_messages = create_message_map("en.json", &[("Common.ok", "OK")]);
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.ok", "OK")]),
        );

        let key_usages = KeyUsageMap::new();
        let mut key_disable_map = KeyDisableMap::new();
        key_disable_map.insert(
            "Common.ok".to_string(),
            KeyDisableStats {
                total_usages: 3,
                disabled_usages: 2, // Not all disabled
            },
        );

        let issues = check_untranslated(
            "en",
            &primary_messages,
            &all_messages,
            &key_usages,
            &key_disable_map,
        );
        // Should report because not all usages are disabled
        assert_eq!(issues.len(), 1);
    }
}

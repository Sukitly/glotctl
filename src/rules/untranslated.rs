//! Untranslated value detection rule.
//!
//! Detects translation values that are identical to the primary locale,
//! which may indicate that the text was not translated.
//!
//! Output format is consistent with replica-lag:
//! - Points to primary locale file (source of truth)
//! - Shows which locales have identical values
//! - Shows where the key is used in code

use crate::{
    analysis::collect::SuppressibleRule,
    analysis::CheckContext,
    analysis::ResolvedKeyUsage,
    analysis::{AllLocaleMessages, LocaleMessages, MessageContext, MessageLocation},
    issues::UntranslatedIssue,
    rules::{build_key_usage_map, helpers::KeyUsageMap},
    utils::contains_alphabetic,
};

pub fn check_untranslated_issues(ctx: &CheckContext) -> Vec<UntranslatedIssue> {
    let primary_locale = &ctx.config.primary_locale;
    let primary_messages = &ctx.messages().primary_messages;
    let all_messages = &ctx.messages().all_messages;
    let key_usages = ctx.all_key_usages();
    let key_usages_map = build_key_usage_map(key_usages);
    check_untranslated(
        primary_locale,
        primary_messages,
        all_messages,
        &key_usages_map,
    )
}

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
///
/// # Returns
/// Vector of UntranslatedIssue for keys with identical values across locales
pub fn check_untranslated(
    primary_locale: &str,
    primary_messages: &LocaleMessages,
    all_messages: &AllLocaleMessages,
    key_usages: &KeyUsageMap,
) -> Vec<UntranslatedIssue> {
    let mut issues = Vec::new();

    for (key, primary_entry) in &primary_messages.entries {
        // Skip if value has no alphabetic characters (pure numbers/symbols)
        if !contains_alphabetic(&primary_entry.context.value) {
            continue;
        }

        // Get all usages and filter out those with untranslated rule suppressed
        let all_usages = key_usages.get(key).map(|v| v.as_slice()).unwrap_or(&[]);
        let non_suppressed: Vec<ResolvedKeyUsage> = all_usages
            .iter()
            .filter(|u| !u.suppressed_rules.contains(&SuppressibleRule::Untranslated))
            .cloned()
            .collect();

        // Skip if all usages have untranslated rule disabled
        // (one-vote-veto: if ANY usage is not disabled, report the issue)
        if non_suppressed.is_empty() && !all_usages.is_empty() {
            continue;
        }

        // Collect locales with identical values
        let mut identical_in: Vec<String> = all_messages
            .iter()
            .filter(|(locale, msgs)| {
                *locale != primary_locale
                    && msgs.get(key).map(|e| &e.context.value) == Some(&primary_entry.context.value)
            })
            .map(|(locale, _)| locale.clone())
            .collect();
        identical_in.sort();

        if !identical_in.is_empty() {
            let usages: Vec<ResolvedKeyUsage> = non_suppressed.into_iter().collect();

            issues.push(UntranslatedIssue {
                context: MessageContext::new(
                    MessageLocation::new(
                        &primary_entry.context.location.file_path,
                        primary_entry.context.location.line,
                        1,
                    ),
                    key.clone(),
                    primary_entry.context.value.clone(),
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
    use std::collections::{HashMap, HashSet};

    use crate::rules::untranslated::*;
    use crate::{
        analysis::FullKey,
        analysis::{CommentStyle, SourceContext, SourceLocation},
        analysis::{LocaleMessages, MessageContext, MessageEntry, MessageLocation, ValueType},
    };

    fn create_message_map(file: &str, entries: &[(&str, &str)]) -> LocaleMessages {
        let locale = file.trim_end_matches(".json");
        let mut messages = LocaleMessages::new(locale, file);
        for (i, (k, v)) in entries.iter().enumerate() {
            messages.entries.insert(
                k.to_string(),
                MessageEntry {
                    context: MessageContext::new(
                        MessageLocation::with_line(file, i + 1),
                        k.to_string(),
                        v.to_string(),
                    ),
                    value_type: ValueType::String,
                },
            );
        }
        messages
    }

    fn make_usage(key: &str, file: &str, line: usize) -> ResolvedKeyUsage {
        ResolvedKeyUsage {
            key: FullKey::new(key),
            context: SourceContext::new(
                SourceLocation::new(file, line, 1),
                format!("t('{}')", key),
                CommentStyle::Jsx,
            ),
            suppressed_rules: HashSet::new(),
            from_schema: None,
        }
    }

    fn make_suppressed_usage(key: &str, file: &str, line: usize) -> ResolvedKeyUsage {
        ResolvedKeyUsage {
            key: FullKey::new(key),
            context: SourceContext::new(
                SourceLocation::new(file, line, 1),
                format!("t('{}')", key),
                CommentStyle::Jsx,
            ),
            suppressed_rules: [SuppressibleRule::Untranslated].into_iter().collect(),
            from_schema: None,
        }
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

        let issues = check_untranslated("en", &primary_messages, &all_messages, &key_usages);
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

        let issues = check_untranslated("en", &primary_messages, &all_messages, &key_usages);
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

        let issues = check_untranslated("en", &primary_messages, &all_messages, &key_usages);
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

        let issues = check_untranslated("en", &primary_messages, &all_messages, &key_usages);
        // Should skip because value has no alphabetic characters
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_untranslated_skips_all_suppressed() {
        let primary_messages = create_message_map("en.json", &[("Common.ok", "OK")]);
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.ok", "OK")]),
        );

        let mut key_usages = KeyUsageMap::new();
        key_usages.insert(
            "Common.ok".to_string(),
            vec![
                make_suppressed_usage("Common.ok", "a.tsx", 10),
                make_suppressed_usage("Common.ok", "b.tsx", 20),
            ],
        );

        let issues = check_untranslated("en", &primary_messages, &all_messages, &key_usages);
        // Should skip because all usages are suppressed
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_untranslated_reports_non_suppressed_usages() {
        let primary_messages = create_message_map("en.json", &[("Common.ok", "OK")]);
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.ok", "OK")]),
        );

        let mut key_usages = KeyUsageMap::new();
        key_usages.insert(
            "Common.ok".to_string(),
            vec![
                make_suppressed_usage("Common.ok", "a.tsx", 10), // suppressed
                make_usage("Common.ok", "b.tsx", 20),            // not suppressed
                make_suppressed_usage("Common.ok", "c.tsx", 30), // suppressed
            ],
        );

        let issues = check_untranslated("en", &primary_messages, &all_messages, &key_usages);
        // Should report because some usages are not suppressed
        assert_eq!(issues.len(), 1);
        // Should only include the non-suppressed usage
        assert_eq!(issues[0].usages.len(), 1);
        assert_eq!(issues[0].usages[0].context.file_path(), "b.tsx");
    }
}

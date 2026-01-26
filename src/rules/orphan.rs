//! Orphan translation key detection rule.
//!
//! Detects translation keys that exist in non-primary locales
//! but are missing from the primary locale.

use std::collections::HashMap;

use crate::{
    analysis::CheckContext,
    analysis::{MessageContext, MessageLocation},
    issues::OrphanKeyIssue,
    parsers::json::MessageMap,
};

pub fn check_orphan_keys_issues(ctx: &CheckContext) -> Vec<OrphanKeyIssue> {
    let primary_locale = &ctx.config.primary_locale;
    let all_messages = &ctx.messages().all_messages;
    check_orphan_keys(primary_locale, all_messages)
}

/// Check for orphan translation keys.
///
/// Finds all keys that exist in non-primary locales but are missing from
/// the primary locale. These are typically leftover keys from deleted features
/// that were removed from the primary locale but not from other locales.
///
/// # Arguments
/// * `primary_locale` - The primary locale code (e.g., "en")
/// * `all_messages` - All messages from all locales
///
/// # Returns
/// Vector of OrphanKeyIssue for keys missing in primary locale
pub fn check_orphan_keys(
    primary_locale: &str,
    all_messages: &HashMap<String, MessageMap>,
) -> Vec<OrphanKeyIssue> {
    let Some(primary_messages) = all_messages.get(primary_locale) else {
        return Vec::new();
    };

    let mut issues: Vec<OrphanKeyIssue> = all_messages
        .iter()
        .filter(|(locale, _)| *locale != primary_locale)
        .flat_map(|(locale, messages)| {
            messages
                .iter()
                .filter(|(key, _)| !primary_messages.contains_key(*key))
                .map(|(key, entry)| OrphanKeyIssue {
                    context: MessageContext::new(
                        MessageLocation::new(&entry.file_path, entry.line, 1),
                        key.clone(),
                        entry.value.clone(),
                    ),
                    locale: locale.clone(),
                })
        })
        .collect();

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
    use crate::parsers::json::{MessageEntry, ValueType};
    use crate::rules::orphan::*;

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
    fn test_check_orphan_key_none_orphan() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map("en.json", &[("Common.submit", "Submit")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.submit", "提交")]),
        );

        let issues = check_orphan_keys("en", &all_messages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_orphan_key_one_orphan() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map("en.json", &[("Common.submit", "Submit")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map(
                "zh.json",
                &[("Common.submit", "提交"), ("Common.orphan", "孤儿")],
            ),
        );

        let issues = check_orphan_keys("en", &all_messages);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].context.key, "Common.orphan");
        assert_eq!(issues[0].locale, "zh");
    }

    #[test]
    fn test_check_orphan_key_multiple_locales() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map("en.json", &[("Common.submit", "Submit")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map(
                "zh.json",
                &[("Common.submit", "提交"), ("Common.orphan1", "孤儿1")],
            ),
        );
        all_messages.insert(
            "ja".to_string(),
            create_message_map(
                "ja.json",
                &[("Common.submit", "送信"), ("Common.orphan2", "孤児2")],
            ),
        );

        let issues = check_orphan_keys("en", &all_messages);
        assert_eq!(issues.len(), 2);

        let keys: Vec<_> = issues.iter().map(|i| i.context.key.as_str()).collect();
        assert!(keys.contains(&"Common.orphan1"));
        assert!(keys.contains(&"Common.orphan2"));
    }

    #[test]
    fn test_check_orphan_key_primary_not_found() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "zh".to_string(),
            create_message_map("zh.json", &[("Common.submit", "提交")]),
        );

        let issues = check_orphan_keys("en", &all_messages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_orphan_key_only_primary() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map("en.json", &[("Common.submit", "Submit")]),
        );

        let issues = check_orphan_keys("en", &all_messages);
        assert!(issues.is_empty());
    }
}

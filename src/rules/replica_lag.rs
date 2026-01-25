//! Replica lag detection rule.
//!
//! Detects translation keys that exist in the primary locale but are missing
//! in one or more other locales.

use std::collections::HashMap;

use crate::{
    commands::context::CheckContext,
    parsers::json::MessageMap,
    rules::helpers::{KeyUsageMap, build_key_usage_map, get_usages_for_key},
    types::{
        context::{MessageContext, MessageLocation},
        issue::ReplicaLagIssue,
    },
};

pub fn check_replica_lag_issues(ctx: &CheckContext) -> Vec<ReplicaLagIssue> {
    let primary_locale = &ctx.config.primary_locale;
    let all_messages = &ctx.messages().all_messages;
    let key_usages = ctx.all_key_usages();
    let key_usages = build_key_usage_map(key_usages);
    check_replica_lags(primary_locale, all_messages, &key_usages)
}

/// Check for replica lag issues.
///
/// Finds all keys in the primary locale that are missing from other locales.
///
/// # Arguments
/// * `primary_locale` - The primary locale code (e.g., "en")
/// * `all_messages` - All messages from all locales
/// * `key_usages` - Map of key to usage locations (for showing where keys are used)
///
/// # Returns
/// Vector of ReplicaLagIssue for keys missing in other locales
pub fn check_replica_lags(
    primary_locale: &str,
    all_messages: &HashMap<String, MessageMap>,
    key_usages: &KeyUsageMap,
) -> Vec<ReplicaLagIssue> {
    let Some(primary_messages) = all_messages.get(primary_locale) else {
        return Vec::new();
    };

    let mut issues: Vec<ReplicaLagIssue> = primary_messages
        .iter()
        .filter_map(|(key, entry)| {
            // Find all locales that are missing this key
            let mut missing_in: Vec<String> = all_messages
                .iter()
                .filter(|(locale, msgs)| *locale != primary_locale && !msgs.contains_key(key))
                .map(|(locale, _)| locale.clone())
                .collect();
            missing_in.sort();

            if missing_in.is_empty() {
                None
            } else {
                let usages = get_usages_for_key(key_usages, key);

                Some(ReplicaLagIssue {
                    context: MessageContext::new(
                        MessageLocation::new(&entry.file_path, entry.line, 1),
                        key.clone(),
                        entry.value.clone(),
                    ),
                    primary_locale: primary_locale.to_string(),
                    missing_in,
                    usages,
                })
            }
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
    use super::*;
    use crate::parsers::json::{MessageEntry, ValueType};

    fn create_message_map(entries: &[(&str, &str)]) -> MessageMap {
        entries
            .iter()
            .enumerate()
            .map(|(i, (k, v))| {
                (
                    k.to_string(),
                    MessageEntry {
                        value: v.to_string(),
                        value_type: ValueType::String,
                        file_path: "test.json".to_string(),
                        line: i + 1,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn test_check_replica_lag_none_missing() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交")]),
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_replica_lags("en", &all_messages, &key_usages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_replica_lag_one_missing() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit"), ("Common.cancel", "Cancel")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交")]),
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_replica_lags("en", &all_messages, &key_usages);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].context.key, "Common.cancel");
        assert_eq!(issues[0].missing_in, vec!["zh"]);
    }

    #[test]
    fn test_check_replica_lag_multiple_locales_missing() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit")]),
        );
        all_messages.insert("zh".to_string(), create_message_map(&[]));
        all_messages.insert("ja".to_string(), create_message_map(&[]));

        let key_usages = KeyUsageMap::new();
        let issues = check_replica_lags("en", &all_messages, &key_usages);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].context.key, "Common.submit");
        // Should be sorted alphabetically
        assert_eq!(issues[0].missing_in, vec!["ja", "zh"]);
    }

    #[test]
    fn test_check_replica_lag_primary_not_found() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交")]),
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_replica_lags("en", &all_messages, &key_usages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_replica_lag_only_primary() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit")]),
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_replica_lags("en", &all_messages, &key_usages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_replica_lag_sorted_output() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[
                ("Common.zebra", "Zebra"),
                ("Common.apple", "Apple"),
                ("Common.mango", "Mango"),
            ]),
        );
        all_messages.insert("zh".to_string(), create_message_map(&[]));

        let key_usages = KeyUsageMap::new();
        let issues = check_replica_lags("en", &all_messages, &key_usages);

        assert_eq!(issues.len(), 3);
        // Should be sorted by line number (which corresponds to order in create_message_map)
        assert_eq!(issues[0].context.key, "Common.zebra");
        assert_eq!(issues[1].context.key, "Common.apple");
        assert_eq!(issues[2].context.key, "Common.mango");
    }
}

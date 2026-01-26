//! Unused translation key detection rule.
//!
//! Detects translation keys that are defined in the primary locale
//! but never used in any source code.

use std::collections::HashSet;

use crate::{
    analysis::{MessageContext, MessageLocation},
    commands::context::CheckContext,
    issues::UnusedKeyIssue,
    parsers::json::MessageMap,
};

pub fn check_unused_keys_issues(ctx: &CheckContext) -> Vec<UnusedKeyIssue> {
    let used_keys = ctx.used_keys();
    let primary_messages = &ctx.messages().primary_messages;
    check_unused_keys(used_keys, primary_messages)
}

/// Check for unused translation keys.
///
/// Finds all keys defined in the primary locale that are not used in any source code.
///
/// # Arguments
/// * `used_keys` - Set of all translation keys found in source code
/// * `primary_messages` - Messages from the primary locale
///
/// # Returns
/// Vector of UnusedKeyIssue for keys defined but not used
pub fn check_unused_keys(
    used_keys: &HashSet<String>,
    primary_messages: &MessageMap,
) -> Vec<UnusedKeyIssue> {
    let mut issues: Vec<UnusedKeyIssue> = primary_messages
        .iter()
        .filter(|(key, _)| !used_keys.contains(*key))
        .map(|(key, entry)| UnusedKeyIssue {
            context: MessageContext::new(
                MessageLocation::new(&entry.file_path, entry.line, 1),
                key.clone(),
                entry.value.clone(),
            ),
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
    use crate::rules::unused::*;

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
                        file_path: "en.json".to_string(),
                        line: i + 1,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn test_check_unused_key_none_unused() {
        let primary_messages = create_message_map(&[("Common.submit", "Submit")]);
        let used_keys: HashSet<String> = ["Common.submit".to_string()].into_iter().collect();

        let issues = check_unused_keys(&used_keys, &primary_messages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_unused_key_one_unused() {
        let primary_messages =
            create_message_map(&[("Common.submit", "Submit"), ("Common.unused", "Unused")]);
        let used_keys: HashSet<String> = ["Common.submit".to_string()].into_iter().collect();

        let issues = check_unused_keys(&used_keys, &primary_messages);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].context.key, "Common.unused");
        assert_eq!(issues[0].context.value, "Unused");
    }

    #[test]
    fn test_check_unused_key_all_unused() {
        let primary_messages =
            create_message_map(&[("Common.a", "A"), ("Common.b", "B"), ("Common.c", "C")]);
        let used_keys: HashSet<String> = HashSet::new();

        let issues = check_unused_keys(&used_keys, &primary_messages);
        assert_eq!(issues.len(), 3);
    }

    #[test]
    fn test_check_unused_key_empty_messages() {
        let primary_messages = create_message_map(&[]);
        let used_keys: HashSet<String> = ["Common.submit".to_string()].into_iter().collect();

        let issues = check_unused_keys(&used_keys, &primary_messages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_unused_key_sorted_output() {
        let primary_messages = create_message_map(&[
            ("Common.zebra", "Z"),
            ("Common.apple", "A"),
            ("Common.mango", "M"),
        ]);
        let used_keys: HashSet<String> = HashSet::new();

        let issues = check_unused_keys(&used_keys, &primary_messages);
        assert_eq!(issues.len(), 3);
        // Should be sorted by line number
        assert_eq!(issues[0].context.key, "Common.zebra");
        assert_eq!(issues[1].context.key, "Common.apple");
        assert_eq!(issues[2].context.key, "Common.mango");
    }
}

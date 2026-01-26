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

use std::collections::HashMap;

use crate::{
    analysis::CheckContext,
    analysis::{LocaleTypeMismatch, MessageContext, MessageLocation, ValueType},
    issues::TypeMismatchIssue,
    parsers::json::MessageMap,
    rules::{
        build_key_usage_map,
        helpers::{get_usages_for_key, KeyUsageMap},
    },
};

pub fn check_type_mismatch_issues(ctx: &CheckContext) -> Vec<TypeMismatchIssue> {
    let primary_locale = &ctx.config.primary_locale;
    let primary_messages = &ctx.messages().primary_messages;
    let all_messages = &ctx.messages().all_messages;
    let key_usages = ctx.all_key_usages();
    let key_usages_map = build_key_usage_map(key_usages);
    check_type_mismatch(
        primary_locale,
        primary_messages,
        all_messages,
        &key_usages_map,
    )
}

/// Check for type mismatches between locales.
///
/// Finds all keys where the value type (string vs array) differs between
/// the primary locale and other locales.
///
/// # Arguments
/// * `primary_locale` - The primary locale code (e.g., "en")
/// * `primary_messages` - Messages from the primary locale
/// * `all_messages` - All messages from all locales
/// * `key_usages` - Map of key to usage locations (for showing where keys are used)
///
/// # Returns
/// Vector of TypeMismatchIssue for keys with type mismatches
pub fn check_type_mismatch(
    primary_locale: &str,
    primary_messages: &MessageMap,
    all_messages: &HashMap<String, MessageMap>,
    key_usages: &KeyUsageMap,
) -> Vec<TypeMismatchIssue> {
    let mut issues = Vec::new();

    for (key, primary_entry) in primary_messages {
        // Collect locales with type mismatch
        let mut mismatched_in: Vec<LocaleTypeMismatch> = all_messages
            .iter()
            .filter_map(|(locale, msgs)| {
                if locale == primary_locale {
                    return None;
                }
                msgs.get(key).and_then(|entry| {
                    let primary_type = convert_value_type(primary_entry.value_type);
                    let entry_type = convert_value_type(entry.value_type);
                    if entry_type != primary_type {
                        Some(LocaleTypeMismatch::new(
                            locale.clone(),
                            entry_type,
                            MessageLocation::new(&entry.file_path, entry.line, 1),
                        ))
                    } else {
                        None
                    }
                })
            })
            .collect();
        mismatched_in.sort();

        if !mismatched_in.is_empty() {
            let usages = get_usages_for_key(key_usages, key);

            issues.push(TypeMismatchIssue {
                context: MessageContext::new(
                    MessageLocation::new(&primary_entry.file_path, primary_entry.line, 1),
                    key.clone(),
                    primary_entry.value.clone(),
                ),
                expected_type: convert_value_type(primary_entry.value_type),
                primary_locale: primary_locale.to_string(),
                mismatched_in,
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

/// Convert from parsers::json::ValueType to analysis::ValueType
fn convert_value_type(vt: crate::parsers::json::ValueType) -> ValueType {
    match vt {
        crate::parsers::json::ValueType::String => ValueType::String,
        crate::parsers::json::ValueType::StringArray => ValueType::StringArray,
    }
}

#[cfg(test)]
mod tests {
    use crate::parsers::json::{MessageEntry, ValueType as JsonValueType};
    use crate::rules::type_mismatch::*;

    fn create_message_map_with_types(
        file: &str,
        entries: &[(&str, &str, JsonValueType)],
    ) -> MessageMap {
        entries
            .iter()
            .enumerate()
            .map(|(i, (k, v, vt))| {
                (
                    k.to_string(),
                    MessageEntry {
                        value: v.to_string(),
                        value_type: *vt,
                        file_path: file.to_string(),
                        line: i + 1,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn test_check_type_mismatch_none() {
        let primary_messages = create_message_map_with_types(
            "en.json",
            &[("Common.items", "[\"a\", \"b\"]", JsonValueType::StringArray)],
        );
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map_with_types(
                "zh.json",
                &[(
                    "Common.items",
                    "[\"甲\", \"乙\"]",
                    JsonValueType::StringArray,
                )],
            ),
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_type_mismatch("en", &primary_messages, &all_messages, &key_usages);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_check_type_mismatch_one_mismatch() {
        let primary_messages = create_message_map_with_types(
            "en.json",
            &[("Common.items", "[\"a\", \"b\"]", JsonValueType::StringArray)],
        );
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map_with_types(
                "zh.json",
                &[("Common.items", "甲, 乙", JsonValueType::String)], // Wrong type
            ),
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_type_mismatch("en", &primary_messages, &all_messages, &key_usages);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].context.key, "Common.items");
        assert_eq!(issues[0].expected_type, ValueType::StringArray);
        assert_eq!(issues[0].mismatched_in.len(), 1);
        assert_eq!(issues[0].mismatched_in[0].locale, "zh");
        assert_eq!(issues[0].mismatched_in[0].actual_type, ValueType::String);
    }

    #[test]
    fn test_check_type_mismatch_multiple_locales() {
        let primary_messages = create_message_map_with_types(
            "en.json",
            &[("Common.items", "[\"a\"]", JsonValueType::StringArray)],
        );
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map_with_types(
                "zh.json",
                &[("Common.items", "甲", JsonValueType::String)],
            ),
        );
        all_messages.insert(
            "ja".to_string(),
            create_message_map_with_types(
                "ja.json",
                &[("Common.items", "あ", JsonValueType::String)],
            ),
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_type_mismatch("en", &primary_messages, &all_messages, &key_usages);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].mismatched_in.len(), 2);
        // Should be sorted by locale
        assert_eq!(issues[0].mismatched_in[0].locale, "ja");
        assert_eq!(issues[0].mismatched_in[1].locale, "zh");
    }

    #[test]
    fn test_check_type_mismatch_partial() {
        // Only one locale has mismatch
        let primary_messages = create_message_map_with_types(
            "en.json",
            &[("Common.items", "[\"a\"]", JsonValueType::StringArray)],
        );
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map_with_types(
                "zh.json",
                &[("Common.items", "[\"甲\"]", JsonValueType::StringArray)], // Correct type
            ),
        );
        all_messages.insert(
            "ja".to_string(),
            create_message_map_with_types(
                "ja.json",
                &[("Common.items", "あ", JsonValueType::String)], // Wrong type
            ),
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_type_mismatch("en", &primary_messages, &all_messages, &key_usages);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].mismatched_in.len(), 1);
        assert_eq!(issues[0].mismatched_in[0].locale, "ja");
    }

    #[test]
    fn test_check_type_mismatch_ignores_missing_keys() {
        // Key exists in primary but not in replica - not a type mismatch
        let primary_messages = create_message_map_with_types(
            "en.json",
            &[("Common.items", "[\"a\"]", JsonValueType::StringArray)],
        );
        let mut all_messages = HashMap::new();
        all_messages.insert("en".to_string(), primary_messages.clone());
        all_messages.insert(
            "zh".to_string(),
            create_message_map_with_types("zh.json", &[]), // Key missing
        );

        let key_usages = KeyUsageMap::new();
        let issues = check_type_mismatch("en", &primary_messages, &all_messages, &key_usages);
        assert!(issues.is_empty());
    }
}

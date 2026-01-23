use std::collections::{HashMap, HashSet};

use crate::{
    extraction::{KeyExtractionResult as ExtractionResult, UsedKey},
    issue::{
        Issue, KeyUsage, MAX_KEY_USAGES, MessageLocation, OrphanKeyIssue, ReplicaLagIssue,
        SourceLocation, UnusedKeyIssue,
    },
    parsers::json::MessageMap,
};

pub fn find_missing_keys(used_keys: &[UsedKey], messages: &MessageMap) -> Vec<UsedKey> {
    used_keys
        .iter()
        .filter(|key| !messages.contains_key(&key.full_key))
        .cloned()
        .collect()
}

/// Type alias for key usage map: full_key -> list of usage locations
pub type KeyUsageMap = HashMap<String, Vec<KeyUsage>>;

/// Build a map from full_key to all its usage locations across the codebase.
///
/// This is used by replica-lag and untranslated rules to show where keys are referenced.
pub fn build_key_usage_map(extractions: &HashMap<String, ExtractionResult>) -> KeyUsageMap {
    let mut map: KeyUsageMap = HashMap::new();

    for extraction in extractions.values() {
        for used_key in &extraction.used_keys {
            map.entry(used_key.full_key.clone())
                .or_default()
                .push(KeyUsage::new(
                    SourceLocation::new(&used_key.file_path, used_key.line)
                        .with_col(used_key.col)
                        .with_jsx_context(used_key.in_jsx_context),
                ));
        }
    }

    // Sort usages by file path for deterministic output
    for usages in map.values_mut() {
        usages.sort();
    }

    map
}

/// Get usages for a key, limited to max_count.
///
/// Returns (usages, total_count) where usages.len() <= max_count.
pub fn get_usages_for_key(
    key_usages: &KeyUsageMap,
    key: &str,
    max_count: usize,
) -> (Vec<KeyUsage>, usize) {
    match key_usages.get(key) {
        Some(usages) => {
            let total = usages.len();
            let limited: Vec<KeyUsage> = usages.iter().take(max_count).cloned().collect();
            (limited, total)
        }
        None => (Vec::new(), 0),
    }
}

/// Stats about disable comments for a key's usages.
#[derive(Debug, Default, Clone)]
pub struct KeyDisableStats {
    /// Total number of usages for this key.
    pub total_usages: usize,
    /// Number of usages with untranslated rule disabled.
    pub disabled_usages: usize,
}

impl KeyDisableStats {
    /// Returns true if all usages have the untranslated rule disabled.
    pub fn all_disabled(&self) -> bool {
        self.total_usages > 0 && self.disabled_usages == self.total_usages
    }
}

/// Type alias for key disable stats map: full_key -> disable stats
pub type KeyDisableMap = HashMap<String, KeyDisableStats>;

/// Build a map of disable stats for untranslated rule from extractions.
///
/// For each key, tracks how many usages have `glot-disable-next-line untranslated`.
/// A key is fully disabled if ALL its usages have the disable comment.
pub fn build_key_disable_map(extractions: &HashMap<String, ExtractionResult>) -> KeyDisableMap {
    let mut map: KeyDisableMap = HashMap::new();

    for extraction in extractions.values() {
        for used_key in &extraction.used_keys {
            let stats = map.entry(used_key.full_key.clone()).or_default();
            stats.total_usages += 1;
            if used_key.untranslated_disabled {
                stats.disabled_usages += 1;
            }
        }
    }

    map
}

pub fn find_replica_lag(
    primary_locale: &str,
    all_messages: &HashMap<String, MessageMap>,
    key_usages: &KeyUsageMap,
) -> Vec<Issue> {
    let Some(primary_messages) = all_messages.get(primary_locale) else {
        return Vec::new();
    };

    let mut issues: Vec<_> = primary_messages
        .iter()
        .filter_map(|(key, entry)| {
            let mut missing_in: Vec<String> = all_messages
                .iter()
                .filter(|(locale, msgs)| *locale != primary_locale && !msgs.contains_key(key))
                .map(|(locale, _)| locale.clone())
                .collect();
            missing_in.sort();

            if missing_in.is_empty() {
                None
            } else {
                let (usages, total_usages) = get_usages_for_key(key_usages, key, MAX_KEY_USAGES);
                Some(Issue::ReplicaLag(ReplicaLagIssue {
                    location: MessageLocation::new(&entry.file_path, entry.line),
                    key: key.clone(),
                    value: entry.value.clone(),
                    primary_locale: primary_locale.to_string(),
                    missing_in,
                    usages,
                    total_usages,
                }))
            }
        })
        .collect();

    issues.sort();
    issues
}

/// Finds translation keys that are defined but never used in the codebase.
///
/// # Arguments
/// * `all_used_keys` - Set of all translation keys found in source code
/// * `messages` - Map of translation keys to their values
///
/// # Returns
/// Vector of unused keys sorted alphabetically
pub fn find_unused_keys(all_used_keys: &HashSet<String>, messages: &MessageMap) -> Vec<Issue> {
    let mut unused: Vec<_> = messages
        .iter()
        .filter(|(key, _)| !all_used_keys.contains(*key))
        .map(|(key, entry)| {
            Issue::UnusedKey(UnusedKeyIssue {
                location: MessageLocation::new(&entry.file_path, entry.line),
                key: key.clone(),
                value: entry.value.clone(),
            })
        })
        .collect();
    unused.sort();
    unused
}

/// Finds translation keys that are defined in other locales but not in the primary locale.
///
/// # Arguments
/// * `primary_locale` - The primary locale to compare against
/// * `all_messages` - Map of all translation keys to their values
///
/// # Returns
/// Vector of orphan keys sorted alphabetically
pub fn find_orphan_keys(
    primary_locale: &str,
    all_messages: &HashMap<String, MessageMap>,
) -> Vec<Issue> {
    let Some(primary_messages) = all_messages.get(primary_locale) else {
        return Vec::new();
    };

    let mut orphans: Vec<Issue> = all_messages
        .iter()
        .filter(|(locale, _)| *locale != primary_locale)
        .flat_map(|(locale, messages)| {
            messages
                .iter()
                .filter(|(key, _)| !primary_messages.contains_key(*key))
                .map(|(key, entry)| {
                    Issue::OrphanKey(OrphanKeyIssue {
                        location: MessageLocation::new(&entry.file_path, entry.line),
                        key: key.clone(),
                        value: entry.value.clone(),
                        locale: locale.clone(),
                    })
                })
        })
        .collect();

    orphans.sort();
    orphans
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::parsers::jsx::parse_jsx_source;

    fn default_checked_attrs() -> Vec<String> {
        vec![
            "placeholder".to_string(),
            "title".to_string(),
            "alt".to_string(),
            "aria-label".to_string(),
        ]
    }

    fn empty_ignore_texts() -> HashSet<String> {
        HashSet::new()
    }

    /// Parse code and check for hardcoded text.
    fn check_code(
        code: &str,
        attrs: &[String],
        ignore_texts: &HashSet<String>,
    ) -> Vec<crate::issue::HardcodedIssue> {
        use crate::commands::context::Registries;
        use crate::extraction::collect::FileImports;
        use crate::extraction::extract::FileAnalyzer;

        let parsed = parse_jsx_source(code.to_string(), "test.tsx").unwrap();
        let registries = Registries {
            schema: Default::default(),
            key_object: Default::default(),
            key_array: Default::default(),
            string_array: Default::default(),
            translation_prop: Default::default(),
            translation_fn_call: Default::default(),
            default_exports: HashMap::new(),
        };
        let imports = FileImports::new();
        let available_keys = HashSet::new();

        let analyzer = FileAnalyzer::new(
            "test.tsx",
            &parsed.source_map,
            &parsed.comments,
            attrs,
            ignore_texts,
            &registries,
            &imports,
            &parsed.source,
            &available_keys,
        );
        let result = analyzer.analyze(&parsed.module);
        result.hardcoded_issues
    }

    #[test]
    fn it_should_parse_code() {
        let code = r#"<div>Hello World</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_expr_string() {
        let code = r#"<div>{"Submit"}</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues.first().unwrap().text, "Submit");
    }

    #[test]
    fn test_logical_and() {
        let code = r#"<div>{error && "Something wrong"}</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Something wrong");
    }

    #[test]
    fn test_ternary() {
        let code = r#"<div>{loading ? "Loading..." : "Done"}</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_placeholder_attr() {
        let code = r#"<input placeholder="Enter your name" />"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Enter your name");
    }

    #[test]
    fn test_non_checked_attr() {
        let code = r#"<div className="container">Hello</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Hello");
    }

    #[test]
    fn test_non_checked_attr_with_expression() {
        // className with template literal should NOT be checked for hardcoded text
        let code = r#"
            export function Component() {
                const isActive = true;
                return (
                    <div className={`flex-shrink-0 ${isActive ? "text-green" : "text-red"}`}>
                        <span title="Tooltip text">Content</span>
                    </div>
                );
            }
        "#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        // Should only detect "Tooltip text" in title (checked attr) and "Content" in text
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().any(|i| i.text == "Tooltip text"));
        assert!(issues.iter().any(|i| i.text == "Content"));
        // Should NOT detect "flex-shrink-0", "text-green", or "text-red"
        assert!(!issues.iter().any(|i| i.text.contains("flex-shrink")));
        assert!(!issues.iter().any(|i| i.text.contains("text-green")));
        assert!(!issues.iter().any(|i| i.text.contains("text-red")));
    }

    #[test]
    fn test_ignore_pure_numbers() {
        let code = r#"<div>123</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert!(issues.is_empty());
    }

    #[test]
    fn test_ignore_pure_symbols() {
        let code = r#"<div>---</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert!(issues.is_empty());
    }

    #[test]
    fn test_detect_chinese() {
        let code = r#"<div>你好</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_template_literal() {
        let code = r#"<div>{`Hello ${name}`}</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Hello ");
    }

    #[test]
    fn test_multiline_jsx_text() {
        let code = r#"<div>
  Hello
  World
</div>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_jsx_fragment() {
        let code = r#"<>text node</>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "text node");
    }

    #[test]
    fn test_custom_checked_attributes() {
        let code = r#"<input placeholder="Name" title="Input" />"#;
        let attrs = vec!["placeholder".to_string()];
        let issues = check_code(code, &attrs, &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Name");
    }

    #[test]
    fn test_empty_checked_attributes() {
        let code = r#"<input placeholder="Name" />"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert!(issues.is_empty());
    }

    #[test]
    fn test_glot_disable_file() {
        let code = r#"// glot-disable
const x = <div>Ignored</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert!(issues.is_empty());
    }

    #[test]
    fn test_glot_disable_file_jsx_comment() {
        let code = r#"// glot-disable
const x = <>{/* comment */}<div>Ignored</div></>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert!(issues.is_empty());
    }

    #[test]
    fn test_glot_disable_next_line() {
        let code = r#"// glot-disable-next-line
const a = <div>Ignored</div>
const b = <div>Detected</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_glot_disable_next_line_jsx_comment() {
        let code = r#"const x = <>
{/* glot-disable-next-line */}
<div>Ignored</div>
<div>Detected</div>
</>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_glot_disable_next_line_attribute() {
        let code = r#"const x = <>
{/* glot-disable-next-line */}
<input placeholder="Ignored" />
<input placeholder="Detected" />
</>"#;
        let issues = check_code(code, &default_checked_attrs(), &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_glot_disable_next_line_multiple() {
        let code = r#"const x = <>
{/* glot-disable-next-line */}
<div>Ignored 1</div>
<div>Detected 1</div>
{/* glot-disable-next-line */}
<div>Ignored 2</div>
<div>Detected 2</div>
</>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].text, "Detected 1");
        assert_eq!(issues[1].text, "Detected 2");
    }

    #[test]
    fn test_glot_disable_enable_block() {
        let code = r#"const a = <div>Detected 1</div>
// glot-disable
const b = <div>Ignored</div>
// glot-enable
const c = <div>Detected 2</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].text, "Detected 1");
        assert_eq!(issues[1].text, "Detected 2");
    }

    #[test]
    fn test_glot_disable_not_at_first_line() {
        let code = r#"// eslint-disable
// glot-disable
const a = <div>Ignored</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert!(issues.is_empty());
    }

    #[test]
    fn test_glot_disable_without_enable() {
        let code = r#"const a = <div>Detected</div>
// glot-disable
const b = <div>Ignored 1</div>
const c = <div>Ignored 2</div>
const d = <div>Ignored 3</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_glot_enable_without_disable() {
        let code = r#"const a = <div>Detected 1</div>
// glot-enable
const b = <div>Detected 2</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_multiple_disable_enable_blocks() {
        let code = r#"const a = <div>Detected 1</div>
// glot-disable
const b = <div>Ignored 1</div>
// glot-enable
const c = <div>Detected 2</div>
// glot-disable
const d = <div>Ignored 2</div>
// glot-enable
const e = <div>Detected 3</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].text, "Detected 1");
        assert_eq!(issues[1].text, "Detected 2");
        assert_eq!(issues[2].text, "Detected 3");
    }

    #[test]
    fn test_glot_disable_enable_jsx_comment() {
        let code = r#"const x = <>
<div>Detected 1</div>
{/* glot-disable */}
<div>Ignored</div>
{/* glot-enable */}
<div>Detected 2</div>
</>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].text, "Detected 1");
        assert_eq!(issues[1].text, "Detected 2");
    }

    #[test]
    fn test_glot_enable_same_line_code() {
        // Test that code on the same line as glot-enable is detected
        let code = r#"const x = <>
{/* glot-disable */}
<div>Ignored</div>
<div>Detected</div>{/* glot-enable */}
</>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_ignore_texts() {
        let code = r#"<><div>Github</div><div>Hello</div></>"#;
        let ignore_texts: HashSet<String> = ["Github".to_string()].iter().cloned().collect();
        let issues = check_code(code, &[], &ignore_texts);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Hello");
    }

    #[test]
    fn test_ignore_texts_case_sensitive() {
        let code = r#"<div>github</div>"#;
        let ignore_texts: HashSet<String> = ["Github".to_string()].iter().cloned().collect();
        let issues = check_code(code, &[], &ignore_texts);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "github");
    }

    fn create_message_map(entries: &[(&str, &str)]) -> MessageMap {
        use crate::parsers::json::{MessageEntry, ValueType};
        entries
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    MessageEntry {
                        value: v.to_string(),
                        value_type: ValueType::String,
                        file_path: "test.json".to_string(),
                        line: 1,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn test_find_missing_in_other_locales() {
        use crate::issue::IssueReport;

        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit"), ("Common.cancel", "Cancel")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交")]), // missing cancel
        );

        let key_usages = KeyUsageMap::new();
        let missing = find_replica_lag("en", &all_messages, &key_usages);

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].message(), "Common.cancel");
        assert_eq!(
            missing[0].format_details(),
            Some("(\"Cancel\") missing in: zh".to_string())
        );
    }

    #[test]
    fn test_missing_in_multiple_locales() {
        use crate::issue::IssueReport;

        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit")]),
        );
        all_messages.insert("zh".to_string(), create_message_map(&[]));
        all_messages.insert("ja".to_string(), create_message_map(&[]));

        let key_usages = KeyUsageMap::new();
        let missing = find_replica_lag("en", &all_messages, &key_usages);

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].message(), "Common.submit");
        let details = missing[0].format_details().unwrap();
        assert!(details.contains("ja"));
        assert!(details.contains("zh"));
    }

    #[test]
    fn test_no_missing_when_all_locales_have_key() {
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
        let missing = find_replica_lag("en", &all_messages, &key_usages);

        assert!(missing.is_empty());
    }

    #[test]
    fn test_primary_locale_not_found() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交")]),
        );

        let key_usages = KeyUsageMap::new();
        let missing = find_replica_lag("en", &all_messages, &key_usages);

        assert!(missing.is_empty());
    }

    #[test]
    fn test_only_primary_locale() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit")]),
        );

        let key_usages = KeyUsageMap::new();
        let missing = find_replica_lag("en", &all_messages, &key_usages);

        assert!(missing.is_empty());
    }

    #[test]
    fn test_results_sorted_by_key() {
        use crate::issue::IssueReport;

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
        let missing = find_replica_lag("en", &all_messages, &key_usages);

        assert_eq!(missing.len(), 3);
        assert_eq!(missing[0].message(), "Common.apple");
        assert_eq!(missing[1].message(), "Common.mango");
        assert_eq!(missing[2].message(), "Common.zebra");
    }

    // Tests for find_unused_keys
    #[test]
    fn test_find_unused_keys_basic() {
        use crate::issue::IssueReport;

        let messages = create_message_map(&[
            ("Common.submit", "Submit"),
            ("Common.cancel", "Cancel"),
            ("Common.save", "Save"),
        ]);
        let used_keys: HashSet<String> = ["Common.submit".to_string()].into_iter().collect();

        let unused = find_unused_keys(&used_keys, &messages);

        assert_eq!(unused.len(), 2);
        let keys: Vec<_> = unused.iter().map(|i| i.message()).collect();
        assert!(keys.contains(&"Common.cancel"));
        assert!(keys.contains(&"Common.save"));
    }

    #[test]
    fn test_find_unused_keys_all_used() {
        let messages = create_message_map(&[("Common.submit", "Submit")]);
        let used_keys: HashSet<String> = ["Common.submit".to_string()].into_iter().collect();

        let unused = find_unused_keys(&used_keys, &messages);

        assert!(unused.is_empty());
    }

    #[test]
    fn test_find_unused_keys_empty_messages() {
        let messages = create_message_map(&[]);
        let used_keys: HashSet<String> = ["Common.submit".to_string()].into_iter().collect();

        let unused = find_unused_keys(&used_keys, &messages);

        assert!(unused.is_empty());
    }

    #[test]
    fn test_find_unused_keys_empty_used() {
        use crate::issue::IssueReport;

        let messages = create_message_map(&[("Common.submit", "Submit")]);
        let used_keys: HashSet<String> = HashSet::new();

        let unused = find_unused_keys(&used_keys, &messages);

        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].message(), "Common.submit");
        assert_eq!(unused[0].format_details(), Some("(\"Submit\")".to_string()));
    }

    // Tests for find_orphan_keys
    #[test]
    fn test_find_orphan_keys_basic() {
        use crate::issue::IssueReport;

        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交"), ("Common.oldKey", "旧的")]),
        );

        let orphans = find_orphan_keys("en", &all_messages);

        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].message(), "Common.oldKey");
        assert_eq!(
            orphans[0].format_details(),
            Some("in zh (\"旧的\")".to_string())
        );
    }

    #[test]
    fn test_find_orphan_keys_multiple_locales() {
        use crate::issue::IssueReport;

        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交"), ("Common.orphan1", "孤儿1")]),
        );
        all_messages.insert(
            "ja".to_string(),
            create_message_map(&[("Common.submit", "送信"), ("Common.orphan2", "孤児2")]),
        );

        let orphans = find_orphan_keys("en", &all_messages);

        assert_eq!(orphans.len(), 2);
        let keys: Vec<_> = orphans.iter().map(|i| i.message()).collect();
        assert!(keys.contains(&"Common.orphan1"));
        assert!(keys.contains(&"Common.orphan2"));
        // Check that details contain the locale info
        for orphan in &orphans {
            let details = orphan.format_details().unwrap();
            if orphan.message() == "Common.orphan1" {
                assert!(details.contains("zh"));
            } else if orphan.message() == "Common.orphan2" {
                assert!(details.contains("ja"));
            }
        }
    }

    #[test]
    fn test_find_orphan_keys_no_orphans() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "en".to_string(),
            create_message_map(&[("Common.submit", "Submit")]),
        );
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交")]),
        );

        let orphans = find_orphan_keys("en", &all_messages);

        assert!(orphans.is_empty());
    }

    #[test]
    fn test_find_orphan_keys_primary_not_found() {
        let mut all_messages = HashMap::new();
        all_messages.insert(
            "zh".to_string(),
            create_message_map(&[("Common.submit", "提交")]),
        );

        let orphans = find_orphan_keys("en", &all_messages);

        assert!(orphans.is_empty());
    }

    // ============================================================
    // Rule-Specific Disable Tests
    // ============================================================

    #[test]
    fn test_glot_disable_next_line_hardcoded_only() {
        // glot-disable-next-line hardcoded should disable hardcoded rule
        let code = r#"// glot-disable-next-line hardcoded
const a = <div>Ignored</div>
const b = <div>Detected</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_glot_disable_next_line_untranslated_does_not_affect_hardcoded() {
        // glot-disable-next-line untranslated should NOT disable hardcoded rule
        let code = r#"// glot-disable-next-line untranslated
const a = <div>Detected 1</div>
const b = <div>Detected 2</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        // Both should be detected since untranslated doesn't affect hardcoded
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].text, "Detected 1");
        assert_eq!(issues[1].text, "Detected 2");
    }

    #[test]
    fn test_glot_disable_enable_block_hardcoded_only() {
        // glot-disable hardcoded / glot-enable hardcoded should work
        let code = r#"const a = <div>Detected 1</div>
// glot-disable hardcoded
const b = <div>Ignored</div>
// glot-enable hardcoded
const c = <div>Detected 2</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].text, "Detected 1");
        assert_eq!(issues[1].text, "Detected 2");
    }

    #[test]
    fn test_glot_disable_untranslated_does_not_affect_hardcoded_range() {
        // glot-disable untranslated / glot-enable untranslated should NOT affect hardcoded
        let code = r#"const a = <div>Detected 1</div>
// glot-disable untranslated
const b = <div>Detected 2</div>
// glot-enable untranslated
const c = <div>Detected 3</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        // All should be detected since untranslated doesn't affect hardcoded
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].text, "Detected 1");
        assert_eq!(issues[1].text, "Detected 2");
        assert_eq!(issues[2].text, "Detected 3");
    }

    #[test]
    fn test_glot_disable_next_line_both_rules_explicitly() {
        // glot-disable-next-line hardcoded untranslated should disable hardcoded
        let code = r#"// glot-disable-next-line hardcoded untranslated
const a = <div>Ignored</div>
const b = <div>Detected</div>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_glot_disable_next_line_jsx_hardcoded_only() {
        // JSX comment style with hardcoded only
        let code = r#"const x = <>
{/* glot-disable-next-line hardcoded */}
<div>Ignored</div>
<div>Detected</div>
</>"#;
        let issues = check_code(code, &[], &empty_ignore_texts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    // ============================================================
    // KeyDisableStats Tests
    // ============================================================

    #[test]
    fn test_key_disable_stats_all_disabled_true() {
        let stats = KeyDisableStats {
            total_usages: 3,
            disabled_usages: 3,
        };
        assert!(stats.all_disabled());
    }

    #[test]
    fn test_key_disable_stats_all_disabled_false_some_not_disabled() {
        let stats = KeyDisableStats {
            total_usages: 3,
            disabled_usages: 2,
        };
        assert!(!stats.all_disabled());
    }

    #[test]
    fn test_key_disable_stats_all_disabled_false_no_usages() {
        // No usages means not "all disabled" (key is not used at all)
        let stats = KeyDisableStats {
            total_usages: 0,
            disabled_usages: 0,
        };
        assert!(!stats.all_disabled());
    }

    #[test]
    fn test_key_disable_stats_default() {
        let stats = KeyDisableStats::default();
        assert_eq!(stats.total_usages, 0);
        assert_eq!(stats.disabled_usages, 0);
        assert!(!stats.all_disabled());
    }

    #[test]
    fn test_build_key_disable_map_empty() {
        let extractions: HashMap<String, ExtractionResult> = HashMap::new();
        let map = build_key_disable_map(&extractions);
        assert!(map.is_empty());
    }

    #[test]
    fn test_build_key_disable_map_basic() {
        use crate::extraction::UsedKey;

        let mut extractions: HashMap<String, ExtractionResult> = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            ExtractionResult {
                used_keys: vec![
                    UsedKey {
                        full_key: "Common.submit".to_string(),
                        file_path: "test.tsx".to_string(),
                        line: 1,
                        col: 1,
                        source_line: String::new(),
                        in_jsx_context: false,
                        untranslated_disabled: false,
                    },
                    UsedKey {
                        full_key: "Common.submit".to_string(),
                        file_path: "test.tsx".to_string(),
                        line: 2,
                        col: 1,
                        source_line: String::new(),
                        in_jsx_context: false,
                        untranslated_disabled: true, // One usage disabled
                    },
                ],
                warnings: Vec::new(),
                schema_calls: Vec::new(),
                resolved_keys: Vec::new(),
                pattern_warnings: Vec::new(),
            },
        );

        let map = build_key_disable_map(&extractions);
        let stats = map.get("Common.submit").unwrap();
        assert_eq!(stats.total_usages, 2);
        assert_eq!(stats.disabled_usages, 1);
        assert!(!stats.all_disabled()); // Not all disabled
    }

    #[test]
    fn test_build_key_disable_map_all_disabled() {
        use crate::extraction::UsedKey;

        let mut extractions: HashMap<String, ExtractionResult> = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            ExtractionResult {
                used_keys: vec![
                    UsedKey {
                        full_key: "Common.submit".to_string(),
                        file_path: "test.tsx".to_string(),
                        line: 1,
                        col: 1,
                        source_line: String::new(),
                        in_jsx_context: false,
                        untranslated_disabled: true,
                    },
                    UsedKey {
                        full_key: "Common.submit".to_string(),
                        file_path: "test.tsx".to_string(),
                        line: 2,
                        col: 1,
                        source_line: String::new(),
                        in_jsx_context: false,
                        untranslated_disabled: true,
                    },
                ],
                warnings: Vec::new(),
                schema_calls: Vec::new(),
                resolved_keys: Vec::new(),
                pattern_warnings: Vec::new(),
            },
        );

        let map = build_key_disable_map(&extractions);
        let stats = map.get("Common.submit").unwrap();
        assert_eq!(stats.total_usages, 2);
        assert_eq!(stats.disabled_usages, 2);
        assert!(stats.all_disabled()); // All disabled
    }
}

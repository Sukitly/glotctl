use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::Result;

use crate::{
    checkers::{
        extraction::{KeyExtractionResult as ExtractionResult, TranslationKeyVisitor, UsedKey},
        hardcoded::{HardcodedChecker, HardcodedIssue},
        key_objects::FileImports,
    },
    commands::context::Registries,
    issue::{Issue, KeyUsage},
    parsers::{json::MessageMap, jsx::parse_jsx_file},
};

pub use crate::checkers::extraction::KeyExtractionResult;

pub fn check_hardcoded(
    file_path: &str,
    checked_attributes: &[String],
    ignore_texts: &HashSet<String>,
) -> Result<Vec<HardcodedIssue>> {
    let parsed_jsx = parse_jsx_file(Path::new(file_path))?;
    let checker = HardcodedChecker::new(
        file_path,
        checked_attributes,
        ignore_texts,
        &parsed_jsx.source_map,
        &parsed_jsx.comments,
    );
    Ok(checker.check(&parsed_jsx.module))
}

/// Extract translation keys from a source file.
///
/// This function parses the file and extracts all translation key usages.
pub fn extract_translation_keys(
    file_path: &str,
    registries: &Registries,
    file_imports: &FileImports,
    available_keys: &HashSet<String>,
) -> Result<KeyExtractionResult> {
    let parsed_jsx = parse_jsx_file(Path::new(file_path))?;
    let visitor = TranslationKeyVisitor::new(
        file_path,
        &parsed_jsx.source_map,
        registries,
        file_imports,
        &parsed_jsx.source,
        available_keys,
    );
    Ok(visitor.extract(&parsed_jsx.module))
}

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
                .push(KeyUsage {
                    file_path: used_key.file_path.clone(),
                    line: used_key.line,
                    col: used_key.col,
                });
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

/// Maximum number of usage locations to include in issues
const MAX_USAGES: usize = 3;

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
                let (usages, total_usages) = get_usages_for_key(key_usages, key, MAX_USAGES);
                Some(Issue::replica_lag(
                    key,
                    &entry.value,
                    &entry.file_path,
                    entry.line,
                    &missing_in,
                    usages,
                    total_usages,
                ))
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
        .map(|(key, entry)| Issue::unused_key(key, &entry.value, &entry.file_path, entry.line))
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
                    Issue::orphan_key(key, locale, &entry.value, &entry.file_path, entry.line)
                })
        })
        .collect();

    orphans.sort();
    orphans
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;

    use super::*;

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

    /// Creates a temporary file with the given code and returns its path as a string.
    /// The file will be automatically deleted when the returned TempDir is dropped.
    fn create_temp_file(code: &str) -> Result<(tempfile::TempDir, String)> {
        let temp_dir = tempfile::tempdir()?;
        let file_path = temp_dir.path().join("test.tsx");
        fs::write(&file_path, code)?;
        let path = file_path.to_str().unwrap().to_string();
        Ok((temp_dir, path))
    }

    #[test]
    fn it_should_parse_code() {
        let code = r#"<div>Hello World</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_expr_string() {
        let code = r#"<div>{"Submit"}</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues.first().unwrap().text, "Submit");
    }

    #[test]
    fn test_logical_and() {
        let code = r#"<div>{error && "Something wrong"}</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Something wrong");
    }

    #[test]
    fn test_ternary() {
        let code = r#"<div>{loading ? "Loading..." : "Done"}</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn test_placeholder_attr() {
        let code = r#"<input placeholder="Enter your name" />"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Enter your name");
    }

    #[test]
    fn test_non_checked_attr() {
        let code = r#"<div className="container">Hello</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Hello");
    }

    #[test]
    fn test_ignore_pure_numbers() {
        let code = r#"<div>123</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_ignore_pure_symbols() {
        let code = r#"<div>---</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_detect_chinese() {
        let code = r#"<div>你好</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_template_literal() {
        let code = r#"<div>{`Hello ${name}`}</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Hello ");
    }

    #[test]
    fn test_multiline_jsx_text() {
        let code = r#"<div>
  Hello
  World
</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn test_jsx_fragment() {
        let code = r#"<>text node</>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "text node");
    }
    #[test]
    fn test_custom_checked_attributes() {
        let code = r#"<input placeholder="Name" title="Input" />"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();

        let attrs = vec!["placeholder".to_string()];
        let issues = check_hardcoded(&file_path, &attrs, &empty_ignore_texts()).unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Name");
    }

    #[test]
    fn test_empty_checked_attributes() {
        let code = r#"<input placeholder="Name" />"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();

        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();

        assert!(issues.is_empty());
    }

    #[test]
    fn test_glot_disable_file() {
        let code = r#"// glot-disable
const x = <div>Ignored</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_glot_disable_file_jsx_comment() {
        let code = r#"// glot-disable
const x = <>{/* comment */}<div>Ignored</div></>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_glot_disable_next_line() {
        let code = r#"// glot-disable-next-line
const a = <div>Ignored</div>
const b = <div>Detected</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
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
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
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
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues =
            check_hardcoded(&file_path, &default_checked_attrs(), &empty_ignore_texts()).unwrap();
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
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
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
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].text, "Detected 1");
        assert_eq!(issues[1].text, "Detected 2");
    }

    #[test]
    fn test_glot_disable_not_at_first_line() {
        let code = r#"// eslint-disable
// glot-disable
const a = <div>Ignored</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_glot_disable_without_enable() {
        let code = r#"const a = <div>Detected</div>
// glot-disable
const b = <div>Ignored 1</div>
const c = <div>Ignored 2</div>
const d = <div>Ignored 3</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_glot_enable_without_disable() {
        let code = r#"const a = <div>Detected 1</div>
// glot-enable
const b = <div>Detected 2</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
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
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
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
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
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
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let issues = check_hardcoded(&file_path, &[], &empty_ignore_texts()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Detected");
    }

    #[test]
    fn test_ignore_texts() {
        let code = r#"<><div>Github</div><div>Hello</div></>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let ignore_texts: HashSet<String> = ["Github".to_string()].iter().cloned().collect();
        let issues = check_hardcoded(&file_path, &[], &ignore_texts).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "Hello");
    }

    #[test]
    fn test_ignore_texts_case_sensitive() {
        let code = r#"<div>github</div>"#;
        let (_temp_dir, file_path) = create_temp_file(code).unwrap();
        let ignore_texts: HashSet<String> = ["Github".to_string()].iter().cloned().collect();
        let issues = check_hardcoded(&file_path, &[], &ignore_texts).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].text, "github");
    }

    fn create_message_map(entries: &[(&str, &str)]) -> MessageMap {
        use crate::parsers::json::MessageEntry;
        entries
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    MessageEntry {
                        value: v.to_string(),
                        file_path: "test.json".to_string(),
                        line: 1,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn test_find_missing_in_other_locales() {
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
        assert_eq!(missing[0].message, "Common.cancel");
        assert_eq!(
            missing[0].details,
            Some("(\"Cancel\") missing in: zh".to_string())
        );
    }

    #[test]
    fn test_missing_in_multiple_locales() {
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
        assert_eq!(missing[0].message, "Common.submit");
        let details = missing[0].details.as_ref().unwrap();
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
        assert_eq!(missing[0].message, "Common.apple");
        assert_eq!(missing[1].message, "Common.mango");
        assert_eq!(missing[2].message, "Common.zebra");
    }

    // Tests for find_unused_keys
    #[test]
    fn test_find_unused_keys_basic() {
        let messages = create_message_map(&[
            ("Common.submit", "Submit"),
            ("Common.cancel", "Cancel"),
            ("Common.save", "Save"),
        ]);
        let used_keys: HashSet<String> = ["Common.submit".to_string()].into_iter().collect();

        let unused = find_unused_keys(&used_keys, &messages);

        assert_eq!(unused.len(), 2);
        let messages: Vec<_> = unused.iter().map(|i| i.message.as_str()).collect();
        assert!(messages.contains(&"Common.cancel"));
        assert!(messages.contains(&"Common.save"));
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
        let messages = create_message_map(&[("Common.submit", "Submit")]);
        let used_keys: HashSet<String> = HashSet::new();

        let unused = find_unused_keys(&used_keys, &messages);

        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].message, "Common.submit");
        assert_eq!(unused[0].details, Some("(\"Submit\")".to_string()));
    }

    // Tests for find_orphan_keys
    #[test]
    fn test_find_orphan_keys_basic() {
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
        assert_eq!(orphans[0].message, "Common.oldKey");
        assert_eq!(orphans[0].details, Some("in zh (\"旧的\")".to_string()));
    }

    #[test]
    fn test_find_orphan_keys_multiple_locales() {
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
        let messages: Vec<_> = orphans.iter().map(|i| i.message.as_str()).collect();
        assert!(messages.contains(&"Common.orphan1"));
        assert!(messages.contains(&"Common.orphan2"));
        // Check that details contain the locale info
        for orphan in &orphans {
            let details = orphan.details.as_ref().unwrap();
            if orphan.message == "Common.orphan1" {
                assert!(details.contains("zh"));
            } else if orphan.message == "Common.orphan2" {
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
}

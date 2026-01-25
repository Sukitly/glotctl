//! Helper functions for the check command and related commands.
//!
//! These functions work with the old Issue types from src/issue.rs
//! for backward compatibility with clean.rs.

use std::collections::{HashMap, HashSet};

use crate::{
    issue::{Issue, MessageLocation, OrphanKeyIssue, UnusedKeyIssue},
    parsers::json::MessageMap,
};

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
    use std::collections::HashSet;

    use pretty_assertions::assert_eq;

    use crate::parsers::jsx::parse_jsx_source;
    use std::collections::HashMap;

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
        let imports = crate::extraction::collect::FileImports::new();

        // Collect comments (Phase 1)
        use crate::extraction::collect::CommentCollector;
        let file_comments = CommentCollector::collect(&parsed.comments, &parsed.source_map);

        let analyzer = FileAnalyzer::new(
            "test.tsx",
            &parsed.source_map,
            &file_comments,
            attrs,
            ignore_texts,
            &registries,
            &imports,
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
}

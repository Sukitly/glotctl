//! Comment collector for Phase 1.
//!
//! This module collects all glot comments (suppression directives and key declarations)
//! from a file's SingleThreadedComments during Phase 1. The collected FileComments are then
//! passed to FileAnalyzer in Phase 2 for immediate use, avoiding re-parsing.

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use swc_common::{SourceMap, comments::SingleThreadedComments};

use crate::extraction::collect::types::{
    Declarations, Directive, DisabledRange, FileComments, KeyDeclaration, SuppressibleRule,
    Suppressions,
};
use crate::extraction::utils::{expand_glob_pattern, is_glob_pattern};

// Matches quoted strings: "some.key"
static QUOTED_STRING_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());

/// Collects all glot comments from a file.
pub struct CommentCollector;

impl CommentCollector {
    /// Collect all glot comments from a file in a single pass.
    ///
    /// This performs a single traversal of SWC's parsed comments to extract:
    /// 1. Suppression directives (glot-disable, glot-enable, glot-disable-next-line)
    /// 2. Key declarations (glot-message-keys)
    ///
    /// # Arguments
    /// * `swc_comments` - SWC parsed comments
    /// * `source_map` - Source map for line number lookup
    /// * `available_keys` - Available translation keys for glob expansion
    pub fn collect(
        swc_comments: &SingleThreadedComments,
        source_map: &SourceMap,
        available_keys: &HashSet<String>,
    ) -> FileComments {
        let mut suppressions = Suppressions::default();
        let mut declaration_entries = HashMap::new();

        // Collect and sort all comments by line number
        let (leading, trailing) = swc_comments.borrow_all();
        let mut all_comments: Vec<_> = leading
            .iter()
            .chain(trailing.iter())
            .flat_map(|(_, cmts)| cmts.iter())
            .collect();
        all_comments.sort_by_key(|cmt| source_map.lookup_char_pos(cmt.span.lo).line);

        // Track open disable ranges per rule
        let mut open_ranges: HashMap<SuppressibleRule, usize> = HashMap::new();

        for cmt in all_comments {
            let text = cmt.text.trim();
            let line = source_map.lookup_char_pos(cmt.span.lo).line;

            // Try to parse as suppression directive
            if let Some(directive) = Directive::parse(text) {
                Self::handle_suppression(&mut suppressions, &mut open_ranges, directive, line);
            }
            // Try to parse as message-keys declaration
            else if let Some(decl) = Self::parse_message_keys(text, available_keys) {
                declaration_entries.insert(line, decl);
            }
        }

        // Close any open ranges (extend to end of file)
        Self::close_open_ranges(&mut suppressions, open_ranges);

        FileComments {
            suppressions,
            declarations: Declarations {
                entries: declaration_entries,
            },
        }
    }

    /// Handle a suppression directive by updating suppressions state.
    fn handle_suppression(
        suppressions: &mut Suppressions,
        open_ranges: &mut HashMap<SuppressibleRule, usize>,
        directive: Directive,
        line: usize,
    ) {
        match directive {
            Directive::Disable { rules } => {
                for rule in rules {
                    // Only start a new range if not already open
                    open_ranges.entry(rule).or_insert(line);
                }
            }
            Directive::Enable { rules } => {
                for rule in rules {
                    if let Some(start) = open_ranges.remove(&rule) {
                        let end = line.saturating_sub(1);
                        suppressions
                            .disabled_ranges
                            .entry(rule)
                            .or_default()
                            .push(DisabledRange { start, end });
                    }
                }
            }
            Directive::DisableNextLine { rules } => {
                let next_line = line + 1;
                for rule in rules {
                    suppressions
                        .disabled_lines
                        .entry(rule)
                        .or_default()
                        .insert(next_line);
                }
            }
        }
    }

    /// Close any open disable ranges (extend to end of file).
    fn close_open_ranges(
        suppressions: &mut Suppressions,
        open_ranges: HashMap<SuppressibleRule, usize>,
    ) {
        for (rule, start) in open_ranges {
            suppressions
                .disabled_ranges
                .entry(rule)
                .or_default()
                .push(DisabledRange {
                    start,
                    end: usize::MAX,
                });
        }
    }

    /// Parse glot-message-keys declaration from comment text.
    ///
    /// SWC has already stripped comment delimiters, so text is like:
    /// - " glot-message-keys \"key1\", \"key2\""
    ///
    /// Returns None if not a glot-message-keys comment or no valid patterns found.
    fn parse_message_keys(text: &str, available_keys: &HashSet<String>) -> Option<KeyDeclaration> {
        // Check if this is a glot-message-keys comment
        let rest = text.strip_prefix("glot-message-keys")?;

        // Extract quoted patterns
        let patterns: Vec<String> = QUOTED_STRING_REGEX
            .captures_iter(rest)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        if patterns.is_empty() {
            return None;
        }

        // Validate and filter patterns
        let valid_patterns: Vec<String> = patterns
            .into_iter()
            .filter(|p| is_valid_pattern(p))
            .collect();

        if valid_patterns.is_empty() {
            return None;
        }

        // Expand and categorize patterns
        let mut expanded_keys = Vec::new();
        let mut relative_patterns = Vec::new();

        for pattern in valid_patterns {
            if pattern.starts_with('.') {
                // Relative pattern - store for later expansion with namespace
                relative_patterns.push(pattern);
            } else if is_glob_pattern(&pattern) {
                // Absolute glob pattern - expand immediately
                let expanded = expand_glob_pattern(&pattern, available_keys);
                expanded_keys.extend(expanded);
            } else {
                // Absolute literal pattern - add as-is
                expanded_keys.push(pattern);
            }
        }

        Some(KeyDeclaration {
            keys: expanded_keys,
            relative_patterns,
        })
    }
}

/// Validate a pattern. Returns true if valid, false if invalid.
///
/// Valid patterns:
/// - Absolute: `Namespace.key.path` or `key.path`
/// - Relative: `.key.path` (will be expanded with namespace at runtime)
/// - Glob: `Namespace.features.*` or `.features.*`
///
/// Invalid patterns:
/// - Prefix wildcard: `*.suffix` (not supported)
/// - Empty segments: `foo..bar`
fn is_valid_pattern(pattern: &str) -> bool {
    // Handle relative patterns (starting with `.`)
    let pattern_to_check = if let Some(stripped) = pattern.strip_prefix('.') {
        // For relative patterns, validate the part after the leading `.`
        stripped
    } else {
        pattern
    };

    let segments: Vec<&str> = pattern_to_check.split('.').collect();

    // Check for prefix wildcard pattern like "*.suffix"
    if let Some(first) = segments.first()
        && *first == "*"
        && segments.len() > 1
    {
        return false;
    }

    // Check for empty segments
    if segments.iter().any(|s| s.is_empty()) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::jsx::parse_jsx_source;

    /// Helper to parse source and collect comments
    fn parse_and_collect(source: &str, available_keys: &HashSet<String>) -> FileComments {
        let parsed = parse_jsx_source(source.to_string(), "test.tsx").unwrap();
        CommentCollector::collect(&parsed.comments, &parsed.source_map, available_keys)
    }

    // ============================================================
    // Basic parsing tests
    // ============================================================

    #[test]
    fn test_parse_empty_source() {
        let comments = parse_and_collect("const x = 1;", &HashSet::new());
        assert!(comments.declarations.entries.is_empty());
    }

    #[test]
    fn test_parse_absolute_pattern() {
        let source = r#"
// glot-message-keys "Common.submit"
t(`${key}`)
"#;
        let comments = parse_and_collect(source, &HashSet::new());

        let decl = comments.declarations.get_declaration(3);
        assert!(decl.is_some());
        assert_eq!(decl.unwrap().keys, vec!["Common.submit".to_string()]);
        assert!(decl.unwrap().relative_patterns.is_empty());
    }

    #[test]
    fn test_parse_relative_pattern() {
        let source = r#"
// glot-message-keys ".submit"
t(`${key}`)
"#;
        let comments = parse_and_collect(source, &HashSet::new());

        let decl = comments.declarations.get_declaration(3);
        assert!(decl.is_some());
        assert!(decl.unwrap().keys.is_empty());
        assert_eq!(decl.unwrap().relative_patterns, vec![".submit".to_string()]);
    }

    #[test]
    fn test_parse_glob_pattern_with_available_keys() {
        let mut available_keys = HashSet::new();
        available_keys.insert("Common.btn.submit".to_string());
        available_keys.insert("Common.btn.cancel".to_string());
        available_keys.insert("Other.key".to_string());

        let source = r#"
// glot-message-keys "Common.btn.*"
t(`${key}`)
"#;
        let comments = parse_and_collect(source, &available_keys);

        let decl = comments.declarations.get_declaration(3);
        assert!(decl.is_some());
        let keys = &decl.unwrap().keys;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"Common.btn.submit".to_string()));
        assert!(keys.contains(&"Common.btn.cancel".to_string()));
    }

    #[test]
    fn test_single_key() {
        let source = r#"
// glot-message-keys "Common.submit"
const x = 1;
"#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&2).unwrap();
        assert_eq!(decl.keys, vec!["Common.submit"]);
    }

    #[test]
    fn test_multiple_keys() {
        let source = r#"// glot-message-keys "Status.active", "Status.inactive", "Status.pending""#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(
            decl.keys,
            vec!["Status.active", "Status.inactive", "Status.pending"]
        );
    }

    #[test]
    fn test_glob_pattern() {
        let source = r#"// glot-message-keys "errors.*""#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.keys, Vec::<String>::new()); // Glob with no available keys = empty
    }

    #[test]
    fn test_middle_wildcard() {
        let source = r#"// glot-message-keys "form.*.label""#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.keys, Vec::<String>::new()); // Glob with no available keys = empty
    }

    #[test]
    fn test_prefix_wildcard_skipped() {
        let source = r#"// glot-message-keys "*.title""#;
        let comments = parse_and_collect(source, &HashSet::new());

        // Invalid pattern is skipped, no entry created
        assert_eq!(comments.declarations.entries.len(), 0);
    }

    #[test]
    fn test_multiple_annotations() {
        let source = r#"
// glot-message-keys "errors.*"
function showError() {}

// glot-message-keys "form.*.label", "form.*.placeholder"
function renderForm() {}
"#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 2);
        assert!(comments.declarations.entries.contains_key(&2));
        assert!(comments.declarations.entries.contains_key(&5));
    }

    #[test]
    fn test_no_patterns_skipped() {
        let source = r#"// glot-message-keys"#;
        let comments = parse_and_collect(source, &HashSet::new());

        // No valid patterns - no entry created
        assert_eq!(comments.declarations.entries.len(), 0);
    }

    // ============================================================
    // JSX comment tests
    // ============================================================

    #[test]
    fn test_jsx_comment_single_key() {
        let source = r#"{/* glot-message-keys "Common.submit" */}"#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.keys, vec!["Common.submit"]);
    }

    #[test]
    fn test_jsx_comment_multiple_keys() {
        let source = r#"{/* glot-message-keys "Status.active", "Status.inactive" */}"#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.keys, vec!["Status.active", "Status.inactive"]);
    }

    #[test]
    fn test_jsx_comment_glob_pattern() {
        let source = r#"{/* glot-message-keys "CharacterForm.genderOptions.*" */}"#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.keys, Vec::<String>::new()); // Glob with no available keys = empty
    }

    // ============================================================
    // Relative pattern tests
    // ============================================================

    #[test]
    fn test_relative_pattern_simple() {
        let source = r#"// glot-message-keys ".features.title""#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".features.title"]);
    }

    #[test]
    fn test_relative_pattern_with_glob() {
        let source = r#"// glot-message-keys ".features.*.title""#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".features.*.title"]);
    }

    #[test]
    fn test_relative_pattern_jsx_comment() {
        let source = r#"{/* glot-message-keys ".items.*" */}"#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".items.*"]);
    }

    #[test]
    fn test_mixed_absolute_and_relative_patterns() {
        let source = r#"// glot-message-keys "Common.title", ".features.*""#;
        let comments = parse_and_collect(source, &HashSet::new());

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.keys, vec!["Common.title"]);
        assert_eq!(decl.relative_patterns, vec![".features.*"]);
    }

    // ============================================================
    // Combined suppression + declaration tests
    // ============================================================

    #[test]
    fn test_collect_both_suppression_and_declaration() {
        let source = r#"
// glot-disable-next-line hardcoded
const x = "Hardcoded text";

// glot-message-keys "Common.key1", "Common.key2"
t(`${dynamicKey}`);
"#;
        let comments = parse_and_collect(source, &HashSet::new());

        // Check suppression
        assert!(
            comments
                .suppressions
                .is_suppressed(3, SuppressibleRule::Hardcoded)
        );
        assert!(
            !comments
                .suppressions
                .is_suppressed(3, SuppressibleRule::Untranslated)
        );

        // Check declaration
        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&5).unwrap();
        assert_eq!(decl.keys, vec!["Common.key1", "Common.key2"]);
    }

    #[test]
    fn test_collect_preserves_line_numbers() {
        let source = r#"
// Line 2
// glot-message-keys "Key.A"
// Line 4

// Line 6
// glot-disable-next-line
// Line 8
"#;
        let comments = parse_and_collect(source, &HashSet::new());

        // Declaration on line 3
        assert!(comments.declarations.entries.contains_key(&3));

        // Suppression for line 8 (next line after line 7)
        assert!(
            comments
                .suppressions
                .is_suppressed(8, SuppressibleRule::Hardcoded)
        );
    }
}

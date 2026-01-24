//! Comment collector for Phase 1.
//!
//! This module collects all glot comments (suppression directives and key declarations)
//! from a file's SingleThreadedComments during Phase 1. The collected FileComments are then
//! passed to FileAnalyzer in Phase 2 for immediate use, avoiding re-parsing.

use std::collections::HashMap;
use swc_common::{SourceMap, comments::SingleThreadedComments};

use super::directive::Directive;
use crate::extraction::collect::types::{
    Declarations, DisabledRange, FileComments, SuppressibleRule, Suppressions,
};

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
    pub fn collect(swc_comments: &SingleThreadedComments, source_map: &SourceMap) -> FileComments {
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

            if let Some(directive) = Directive::parse(text) {
                match directive {
                    Directive::Disable { rules } => {
                        for rule in rules {
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
                    Directive::MessageKeys(decl) => {
                        declaration_entries.insert(line, decl);
                    }
                }
            }
        }

        // Close any open ranges (extend to end of file)
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

        FileComments {
            suppressions,
            declarations: Declarations {
                entries: declaration_entries,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::jsx::parse_jsx_source;

    /// Helper to parse source and collect comments
    fn parse_and_collect(source: &str) -> FileComments {
        let parsed = parse_jsx_source(source.to_string(), "test.tsx").unwrap();
        CommentCollector::collect(&parsed.comments, &parsed.source_map)
    }

    // ============================================================
    // Basic parsing tests
    // ============================================================

    #[test]
    fn test_parse_empty_source() {
        let comments = parse_and_collect("const x = 1;");
        assert!(comments.declarations.entries.is_empty());
    }

    #[test]
    fn test_parse_absolute_pattern() {
        let source = r#"
// glot-message-keys "Common.submit"
t(`${key}`)
"#;
        let comments = parse_and_collect(source);

        let decl = comments.declarations.get_declaration(3);
        assert!(decl.is_some());
        assert_eq!(
            decl.unwrap().absolute_patterns,
            vec!["Common.submit".to_string()]
        );
        assert!(decl.unwrap().relative_patterns.is_empty());
    }

    #[test]
    fn test_parse_relative_pattern() {
        let source = r#"
// glot-message-keys ".submit"
t(`${key}`)
"#;
        let comments = parse_and_collect(source);

        let decl = comments.declarations.get_declaration(3);
        assert!(decl.is_some());
        assert!(decl.unwrap().absolute_patterns.is_empty());
        assert_eq!(decl.unwrap().relative_patterns, vec![".submit".to_string()]);
    }

    #[test]
    fn test_parse_glob_pattern() {
        let source = r#"
// glot-message-keys "Common.btn.*"
t(`${key}`)
"#;
        let comments = parse_and_collect(source);

        let decl = comments.declarations.get_declaration(3);
        assert!(decl.is_some());
        assert_eq!(
            decl.unwrap().absolute_patterns,
            vec!["Common.btn.*".to_string()]
        );
        assert!(decl.unwrap().relative_patterns.is_empty());
    }

    #[test]
    fn test_single_key() {
        let source = r#"
// glot-message-keys "Common.submit"
const x = 1;
"#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&2).unwrap();
        assert_eq!(decl.absolute_patterns, vec!["Common.submit"]);
    }

    #[test]
    fn test_multiple_keys() {
        let source = r#"// glot-message-keys "Status.active", "Status.inactive", "Status.pending""#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(
            decl.absolute_patterns,
            vec!["Status.active", "Status.inactive", "Status.pending"]
        );
    }

    #[test]
    fn test_glob_pattern() {
        let source = r#"// glot-message-keys "errors.*""#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.absolute_patterns, vec!["errors.*"]); // Glob patterns stored as-is
    }

    #[test]
    fn test_middle_wildcard() {
        let source = r#"// glot-message-keys "form.*.label""#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.absolute_patterns, vec!["form.*.label"]); // Glob patterns stored as-is
    }

    #[test]
    fn test_prefix_wildcard_skipped() {
        let source = r#"// glot-message-keys "*.title""#;
        let comments = parse_and_collect(source);

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
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 2);
        assert!(comments.declarations.entries.contains_key(&2));
        assert!(comments.declarations.entries.contains_key(&5));
    }

    #[test]
    fn test_no_patterns_skipped() {
        let source = r#"// glot-message-keys"#;
        let comments = parse_and_collect(source);

        // No valid patterns - no entry created
        assert_eq!(comments.declarations.entries.len(), 0);
    }

    // ============================================================
    // JSX comment tests
    // ============================================================

    #[test]
    fn test_jsx_comment_single_key() {
        let source = r#"{/* glot-message-keys "Common.submit" */}"#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.absolute_patterns, vec!["Common.submit"]);
    }

    #[test]
    fn test_jsx_comment_multiple_keys() {
        let source = r#"{/* glot-message-keys "Status.active", "Status.inactive" */}"#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(
            decl.absolute_patterns,
            vec!["Status.active", "Status.inactive"]
        );
    }

    #[test]
    fn test_jsx_comment_glob_pattern() {
        let source = r#"{/* glot-message-keys "CharacterForm.genderOptions.*" */}"#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(
            decl.absolute_patterns,
            vec!["CharacterForm.genderOptions.*"]
        ); // Glob patterns stored as-is
    }

    // ============================================================
    // Relative pattern tests
    // ============================================================

    #[test]
    fn test_relative_pattern_simple() {
        let source = r#"// glot-message-keys ".features.title""#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".features.title"]);
    }

    #[test]
    fn test_relative_pattern_with_glob() {
        let source = r#"// glot-message-keys ".features.*.title""#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".features.*.title"]);
    }

    #[test]
    fn test_relative_pattern_jsx_comment() {
        let source = r#"{/* glot-message-keys ".items.*" */}"#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".items.*"]);
    }

    #[test]
    fn test_mixed_absolute_and_relative_patterns() {
        let source = r#"// glot-message-keys "Common.title", ".features.*""#;
        let comments = parse_and_collect(source);

        assert_eq!(comments.declarations.entries.len(), 1);
        let decl = comments.declarations.entries.get(&1).unwrap();
        assert_eq!(decl.absolute_patterns, vec!["Common.title"]);
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
        let comments = parse_and_collect(source);

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
        assert_eq!(decl.absolute_patterns, vec!["Common.key1", "Common.key2"]);
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
        let comments = parse_and_collect(source);

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

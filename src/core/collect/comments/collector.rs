//! Comment collector for Phase 1.
//!
//! This module collects all glot comments (suppression directives and key declarations)
//! from a file's SingleThreadedComments during Phase 1. The collected FileComments are then
//! passed to FileAnalyzer in Phase 2 for immediate use, avoiding re-parsing.
//!
//! # Consecutive Comment Handling
//!
//! When multiple glot directives appear on consecutive lines, they are merged and applied
//! to the next non-comment line. For example:
//!
//! ```tsx
//! {/* glot-disable-next-line untranslated */}
//! {/* glot-message-keys "Common.*" */}
//! {t(`${key}`)}  // <- Both directives apply to this line
//! ```
//!
//! **Important**: Blank lines break the consecutive comment chain. If there's a blank line
//! between a directive and the target code, the directive will NOT apply to that code.
//!
//! ```tsx
//! {/* glot-disable-next-line untranslated */}
//!
//! {t(`${key}`)}  // <- Directive does NOT apply (blank line breaks chain)
//! ```

use std::collections::{HashMap, HashSet};

/// Maximum number of consecutive comment lines to traverse when looking for
/// the target code line or searching backwards for declarations.
pub const MAX_COMMENT_CHAIN_LINES: usize = 10;
use swc_common::SourceMap;

use crate::core::collect::comments::directive::Directive;
use crate::core::collect::types::{
    Declarations, DisabledRange, FileComments, SuppressibleRule, Suppressions,
};
use crate::core::parsers::jsx::ExtractedComments;

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
    /// * `swc_comments` - Extracted comments from parsing
    /// * `source_map` - Source map for line number lookup
    pub fn collect(swc_comments: &ExtractedComments, source_map: &SourceMap) -> FileComments {
        let mut suppressions = Suppressions::default();
        let mut declaration_entries = HashMap::new();

        // Collect all comments with their line numbers (computed once)
        let (leading, trailing) = swc_comments.borrow_all();
        let mut comments_with_lines: Vec<_> = leading
            .iter()
            .chain(trailing.iter())
            .flat_map(|(_, cmts)| cmts.iter())
            .map(|cmt| {
                let line = source_map.lookup_char_pos(cmt.span.lo).line;
                (line, cmt)
            })
            .collect();

        // Sort by line number
        comments_with_lines.sort_by_key(|(line, _)| *line);

        // Collect all comment line numbers for consecutive comment handling
        let comment_lines: HashSet<usize> =
            comments_with_lines.iter().map(|(line, _)| *line).collect();

        // Track open disable ranges per rule
        let mut open_ranges: HashMap<SuppressibleRule, usize> = HashMap::new();

        for (line, cmt) in comments_with_lines {
            let text = cmt.text.trim();

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
                        // Find the next non-comment line
                        let target_line = Self::find_next_non_comment_line(line, &comment_lines);
                        for rule in rules {
                            suppressions
                                .disabled_lines
                                .entry(rule)
                                .or_default()
                                .insert(target_line);
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
                comment_lines,
            },
        }
    }

    /// Find the next non-comment line after the given line.
    ///
    /// This skips over any consecutive comment lines to find the actual
    /// code line that the directive should apply to. Limited to [`MAX_COMMENT_CHAIN_LINES`]
    /// to avoid traversing too far.
    fn find_next_non_comment_line(line: usize, comment_lines: &HashSet<usize>) -> usize {
        let mut next = line + 1;
        let max_line = line + MAX_COMMENT_CHAIN_LINES;
        // Skip consecutive comment lines (with a reasonable limit to avoid infinite loops)
        while comment_lines.contains(&next) && next < max_line {
            next += 1;
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use crate::core::collect::comments::collector::*;
    use crate::core::parsers::jsx::parse_jsx_source;

    /// Helper to parse source and collect comments
    fn parse_and_collect(source: &str) -> FileComments {
        use std::sync::Arc;
        let source_map = Arc::new(swc_common::SourceMap::default());
        let parsed = parse_jsx_source(source.to_string(), "test.tsx", source_map).unwrap();
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
const x = 1;
"#;
        let comments = parse_and_collect(source);

        // Declaration on line 3
        assert!(comments.declarations.entries.contains_key(&3));

        // Suppression for line 8 (code after glot-disable-next-line on line 7)
        assert!(
            comments
                .suppressions
                .is_suppressed(8, SuppressibleRule::Hardcoded)
        );
    }

    // ============================================================
    // Consecutive comment handling tests
    // ============================================================

    #[test]
    fn test_consecutive_comments_suppression_applies_to_code() {
        // When glot-disable-next-line is followed by another comment,
        // the suppression should apply to the first non-comment line
        //
        // Line numbers in source:
        //   Line 1: (empty)
        //   Line 2: {/* glot-disable-next-line untranslated */}
        //   Line 3: {/* glot-message-keys "Common.*" */}
        //   Line 4: {t(`${key}`)}
        let source = r#"
{/* glot-disable-next-line untranslated */}
{/* glot-message-keys "Common.*" */}
{t(`${key}`)}
"#;
        let comments = parse_and_collect(source);

        // Line 4 should be suppressed (skipping over the comment on line 3)
        assert!(
            comments
                .suppressions
                .is_suppressed(4, SuppressibleRule::Untranslated),
            "Line 4 should be suppressed for Untranslated rule"
        );
    }

    #[test]
    fn test_consecutive_comments_declaration_found() {
        // Declaration should be found when searching backwards through comments
        //
        // Line numbers in source:
        //   Line 1: (empty)
        //   Line 2: {/* glot-disable-next-line untranslated */}
        //   Line 3: {/* glot-message-keys "Common.*" */}  <- declaration
        //   Line 4: {t(`${key}`)}  <- code line, should find declaration on line 3
        let source = r#"
{/* glot-disable-next-line untranslated */}
{/* glot-message-keys "Common.*" */}
{t(`${key}`)}
"#;
        let comments = parse_and_collect(source);

        // Declaration should be accessible for line 4 (t() call)
        let decl = comments.declarations.get_declaration(4);
        assert!(decl.is_some(), "Declaration should be found for line 4");
        assert_eq!(decl.unwrap().absolute_patterns, vec!["Common.*"]);
    }

    #[test]
    fn test_multiple_consecutive_comments() {
        // Multiple glot directives on consecutive lines
        //
        // Line numbers in source:
        //   Line 1: (empty)
        //   Line 2: {/* glot-disable-next-line hardcoded */}
        //   Line 3: {/* glot-disable-next-line untranslated */}
        //   Line 4: {/* glot-message-keys "Status.*" */}  <- declaration
        //   Line 5: {t(`${dynamicKey}`)}  <- code line, both suppressions apply
        let source = r#"
{/* glot-disable-next-line hardcoded */}
{/* glot-disable-next-line untranslated */}
{/* glot-message-keys "Status.*" */}
{t(`${dynamicKey}`)}
"#;
        let comments = parse_and_collect(source);

        // Line 5 should be suppressed for both rules
        assert!(
            comments
                .suppressions
                .is_suppressed(5, SuppressibleRule::Hardcoded),
            "Line 5 should be suppressed for Hardcoded"
        );
        assert!(
            comments
                .suppressions
                .is_suppressed(5, SuppressibleRule::Untranslated),
            "Line 5 should be suppressed for Untranslated"
        );

        // Declaration should be found
        let decl = comments.declarations.get_declaration(5);
        assert!(decl.is_some());
        assert_eq!(decl.unwrap().absolute_patterns, vec!["Status.*"]);
    }

    #[test]
    fn test_non_consecutive_comments_not_merged() {
        // Comments separated by blank lines should NOT be merged.
        // Blank lines are not in comment_lines, so they break the chain.
        //
        // Line numbers in source:
        //   Line 1: (empty)
        //   Line 2: {/* glot-disable-next-line untranslated */}  <- targets line 3
        //   Line 3: (empty)  <- blank line, NOT a comment
        //   Line 4: {t(`${key}`)}  <- NOT suppressed
        let source = r#"
{/* glot-disable-next-line untranslated */}

{t(`${key}`)}
"#;
        let comments = parse_and_collect(source);

        // Line 4 should NOT be suppressed (blank line breaks the chain)
        // The directive on line 2 targets line 3 (the blank line), not line 4
        assert!(
            !comments
                .suppressions
                .is_suppressed(4, SuppressibleRule::Untranslated),
            "Line 4 should not be suppressed due to blank line gap"
        );
    }

    #[test]
    fn test_declaration_found_through_comment_chain() {
        // Declaration above other comments should still be found
        //
        // Line numbers in source:
        //   Line 1: (empty)
        //   Line 2: {/* glot-message-keys "Common.*" */}  <- declaration
        //   Line 3: {/* glot-disable-next-line untranslated */}  <- comment (skipped)
        //   Line 4: {t(`${key}`)}  <- code line, searches back through line 3 to find line 2
        let source = r#"
{/* glot-message-keys "Common.*" */}
{/* glot-disable-next-line untranslated */}
{t(`${key}`)}
"#;
        let comments = parse_and_collect(source);

        // Declaration on line 2 should be found for line 4
        let decl = comments.declarations.get_declaration(4);
        assert!(
            decl.is_some(),
            "Declaration should be found through comment chain"
        );
        assert_eq!(decl.unwrap().absolute_patterns, vec!["Common.*"]);
    }

    #[test]
    fn test_declaration_not_found_with_blank_line_gap() {
        // Declaration separated by blank line should NOT be found.
        // Blank lines are not in comment_lines, so they break the backward search.
        //
        // Line numbers in source:
        //   Line 1: (empty)
        //   Line 2: {/* glot-message-keys "Common.*" */}  <- declaration
        //   Line 3: (empty)  <- blank line, NOT a comment, breaks chain
        //   Line 4: {t(`${key}`)}  <- code line, search stops at line 3
        let source = r#"
{/* glot-message-keys "Common.*" */}

{t(`${key}`)}
"#;
        let comments = parse_and_collect(source);

        // Declaration on line 2 should NOT be found for line 4 (blank line breaks chain)
        let decl = comments.declarations.get_declaration(4);
        assert!(
            decl.is_none(),
            "Declaration should not be found due to blank line gap"
        );
    }
}

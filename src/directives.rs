//! Shared directive parsing for glot comments.
//!
//! Supports rule-specific disable comments:
//! - `glot-disable-next-line` - disable all rules for next line
//! - `glot-disable-next-line hardcoded` - disable only hardcoded rule
//! - `glot-disable-next-line untranslated` - disable only untranslated rule
//! - `glot-disable-next-line hardcoded untranslated` - disable both
//! - `glot-disable` / `glot-enable` - range-based disabling

use std::collections::{HashMap, HashSet};

use swc_common::{SourceMap, comments::SingleThreadedComments};

/// Rules that can be disabled via glot comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisableRule {
    Hardcoded,
    Untranslated,
}

impl DisableRule {
    /// Parse rule name from string (case insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hardcoded" => Some(Self::Hardcoded),
            "untranslated" => Some(Self::Untranslated),
            _ => None,
        }
    }

    /// All available rules (for "no args = all rules" case).
    pub fn all() -> HashSet<Self> {
        [Self::Hardcoded, Self::Untranslated].into_iter().collect()
    }

    /// Get the suffix string for this rule (used in baseline comments).
    /// Reserved for future use when baseline supports multiple rules.
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hardcoded => "hardcoded",
            Self::Untranslated => "untranslated",
        }
    }
}

/// Parsed glot directive.
#[derive(Debug, Clone)]
pub enum Directive {
    Disable { rules: HashSet<DisableRule> },
    Enable { rules: HashSet<DisableRule> },
    DisableNextLine { rules: HashSet<DisableRule> },
}

impl Directive {
    /// Parse directive from comment text.
    /// Returns None if not a glot directive.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();

        // Order matters: check longer prefix first
        if let Some(rest) = text.strip_prefix("glot-disable-next-line") {
            return Some(Self::DisableNextLine {
                rules: Self::parse_rules(rest),
            });
        }
        if let Some(rest) = text.strip_prefix("glot-disable") {
            return Some(Self::Disable {
                rules: Self::parse_rules(rest),
            });
        }
        if let Some(rest) = text.strip_prefix("glot-enable") {
            return Some(Self::Enable {
                rules: Self::parse_rules(rest),
            });
        }

        None
    }

    fn parse_rules(rest: &str) -> HashSet<DisableRule> {
        let rest = rest.trim();
        if rest.is_empty() {
            // No rules specified = all rules
            return DisableRule::all();
        }

        let parsed: HashSet<_> = rest
            .split_whitespace()
            .filter_map(DisableRule::parse)
            .collect();

        // If no valid rules parsed, treat as all rules
        if parsed.is_empty() {
            DisableRule::all()
        } else {
            parsed
        }
    }
}

/// Range representing disabled lines [start, end] inclusive.
#[derive(Debug, Clone, Copy)]
struct DisabledRange {
    start: usize,
    end: usize, // usize::MAX for open-ended
}

/// Tracks disabled lines per rule for a single file.
#[derive(Debug, Default)]
pub struct DisableContext {
    /// Single-line disables: rule -> set of line numbers
    disabled_lines: HashMap<DisableRule, HashSet<usize>>,
    /// Range-based disables: rule -> list of (start, end) ranges
    disabled_ranges: HashMap<DisableRule, Vec<DisabledRange>>,
}

impl DisableContext {
    /// Check if a line should be ignored for a specific rule.
    pub fn should_ignore(&self, line: usize, rule: DisableRule) -> bool {
        // Check single-line disables
        if let Some(lines) = self.disabled_lines.get(&rule)
            && lines.contains(&line)
        {
            return true;
        }
        // Check range disables
        if let Some(ranges) = self.disabled_ranges.get(&rule)
            && ranges.iter().any(|r| line >= r.start && line <= r.end)
        {
            return true;
        }
        false
    }

    /// Build DisableContext from SWC comments.
    pub fn from_comments(comments: &SingleThreadedComments, source_map: &SourceMap) -> Self {
        let mut ctx = Self::default();
        let (leading, trailing) = comments.borrow_all();

        let mut all_comments: Vec<_> = leading
            .iter()
            .chain(trailing.iter())
            .flat_map(|(_, cmts)| cmts.iter())
            .collect();
        all_comments.sort_by_key(|cmt| source_map.lookup_char_pos(cmt.span.lo).line);

        // Track open disable ranges per rule
        let mut open_ranges: HashMap<DisableRule, usize> = HashMap::new();

        for cmt in all_comments {
            let text = cmt.text.trim();
            let loc = source_map.lookup_char_pos(cmt.span.lo);

            if let Some(directive) = Directive::parse(text) {
                match directive {
                    Directive::Disable { rules } => {
                        for rule in rules {
                            // Only start a new range if not already open
                            open_ranges.entry(rule).or_insert(loc.line);
                        }
                    }
                    Directive::Enable { rules } => {
                        for rule in rules {
                            if let Some(start) = open_ranges.remove(&rule) {
                                let end = loc.line.saturating_sub(1);
                                ctx.disabled_ranges
                                    .entry(rule)
                                    .or_default()
                                    .push(DisabledRange { start, end });
                            }
                        }
                    }
                    Directive::DisableNextLine { rules } => {
                        let next_line = loc.line + 1;
                        for rule in rules {
                            ctx.disabled_lines
                                .entry(rule)
                                .or_default()
                                .insert(next_line);
                        }
                    }
                }
            }
        }

        // Close any open ranges (extend to end of file)
        for (rule, start) in open_ranges {
            ctx.disabled_ranges
                .entry(rule)
                .or_default()
                .push(DisabledRange {
                    start,
                    end: usize::MAX,
                });
        }

        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // DisableRule Tests
    // ============================================================

    #[test]
    fn test_disable_rule_parse_hardcoded() {
        assert_eq!(
            DisableRule::parse("hardcoded"),
            Some(DisableRule::Hardcoded)
        );
        assert_eq!(
            DisableRule::parse("HARDCODED"),
            Some(DisableRule::Hardcoded)
        );
        assert_eq!(
            DisableRule::parse("Hardcoded"),
            Some(DisableRule::Hardcoded)
        );
    }

    #[test]
    fn test_disable_rule_parse_untranslated() {
        assert_eq!(
            DisableRule::parse("untranslated"),
            Some(DisableRule::Untranslated)
        );
        assert_eq!(
            DisableRule::parse("UNTRANSLATED"),
            Some(DisableRule::Untranslated)
        );
    }

    #[test]
    fn test_disable_rule_parse_unknown() {
        assert_eq!(DisableRule::parse("unknown"), None);
        assert_eq!(DisableRule::parse(""), None);
        assert_eq!(DisableRule::parse("hard-coded"), None);
    }

    #[test]
    fn test_disable_rule_all() {
        let all = DisableRule::all();
        assert!(all.contains(&DisableRule::Hardcoded));
        assert!(all.contains(&DisableRule::Untranslated));
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_disable_rule_as_str() {
        assert_eq!(DisableRule::Hardcoded.as_str(), "hardcoded");
        assert_eq!(DisableRule::Untranslated.as_str(), "untranslated");
    }

    // ============================================================
    // Directive Tests
    // ============================================================

    #[test]
    fn test_directive_parse_disable_next_line_no_args() {
        let d = Directive::parse("glot-disable-next-line").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(rules.contains(&DisableRule::Untranslated));
                assert_eq!(rules.len(), 2);
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    #[test]
    fn test_directive_parse_disable_next_line_hardcoded() {
        let d = Directive::parse("glot-disable-next-line hardcoded").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(!rules.contains(&DisableRule::Untranslated));
                assert_eq!(rules.len(), 1);
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    #[test]
    fn test_directive_parse_disable_next_line_untranslated() {
        let d = Directive::parse("glot-disable-next-line untranslated").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                assert!(!rules.contains(&DisableRule::Hardcoded));
                assert!(rules.contains(&DisableRule::Untranslated));
                assert_eq!(rules.len(), 1);
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    #[test]
    fn test_directive_parse_disable_next_line_both_rules() {
        let d = Directive::parse("glot-disable-next-line hardcoded untranslated").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(rules.contains(&DisableRule::Untranslated));
                assert_eq!(rules.len(), 2);
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    #[test]
    fn test_directive_parse_disable_next_line_reversed_order() {
        let d = Directive::parse("glot-disable-next-line untranslated hardcoded").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(rules.contains(&DisableRule::Untranslated));
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    #[test]
    fn test_directive_parse_disable_next_line_invalid_rule_falls_back_to_all() {
        let d = Directive::parse("glot-disable-next-line foobar").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                // Invalid rule name falls back to all rules
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(rules.contains(&DisableRule::Untranslated));
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    #[test]
    fn test_directive_parse_disable_next_line_mixed_valid_invalid() {
        let d = Directive::parse("glot-disable-next-line hardcoded foobar").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                // Only valid rule is parsed
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(!rules.contains(&DisableRule::Untranslated));
                assert_eq!(rules.len(), 1);
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    #[test]
    fn test_directive_parse_disable_no_args() {
        let d = Directive::parse("glot-disable").unwrap();
        match d {
            Directive::Disable { rules } => {
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(rules.contains(&DisableRule::Untranslated));
            }
            _ => panic!("expected Disable"),
        }
    }

    #[test]
    fn test_directive_parse_disable_with_rule() {
        let d = Directive::parse("glot-disable hardcoded").unwrap();
        match d {
            Directive::Disable { rules } => {
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(!rules.contains(&DisableRule::Untranslated));
            }
            _ => panic!("expected Disable"),
        }
    }

    #[test]
    fn test_directive_parse_enable_no_args() {
        let d = Directive::parse("glot-enable").unwrap();
        match d {
            Directive::Enable { rules } => {
                assert!(rules.contains(&DisableRule::Hardcoded));
                assert!(rules.contains(&DisableRule::Untranslated));
            }
            _ => panic!("expected Enable"),
        }
    }

    #[test]
    fn test_directive_parse_enable_with_rule() {
        let d = Directive::parse("glot-enable untranslated").unwrap();
        match d {
            Directive::Enable { rules } => {
                assert!(!rules.contains(&DisableRule::Hardcoded));
                assert!(rules.contains(&DisableRule::Untranslated));
            }
            _ => panic!("expected Enable"),
        }
    }

    #[test]
    fn test_directive_parse_not_a_directive() {
        assert!(Directive::parse("some random comment").is_none());
        assert!(Directive::parse("glot-something").is_none());
        assert!(Directive::parse("").is_none());
    }

    #[test]
    fn test_directive_parse_with_whitespace() {
        let d = Directive::parse("  glot-disable-next-line  hardcoded  ").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                assert!(rules.contains(&DisableRule::Hardcoded));
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    // ============================================================
    // DisableContext Tests
    // ============================================================

    #[test]
    fn test_disable_context_default() {
        let ctx = DisableContext::default();
        assert!(!ctx.should_ignore(1, DisableRule::Hardcoded));
        assert!(!ctx.should_ignore(1, DisableRule::Untranslated));
    }

    #[test]
    fn test_disable_context_should_ignore_single_line() {
        let mut ctx = DisableContext::default();
        ctx.disabled_lines
            .entry(DisableRule::Hardcoded)
            .or_default()
            .insert(5);

        assert!(!ctx.should_ignore(4, DisableRule::Hardcoded));
        assert!(ctx.should_ignore(5, DisableRule::Hardcoded));
        assert!(!ctx.should_ignore(6, DisableRule::Hardcoded));

        // Untranslated is not affected
        assert!(!ctx.should_ignore(5, DisableRule::Untranslated));
    }

    #[test]
    fn test_disable_context_should_ignore_range() {
        let mut ctx = DisableContext::default();
        ctx.disabled_ranges
            .entry(DisableRule::Untranslated)
            .or_default()
            .push(DisabledRange { start: 10, end: 20 });

        assert!(!ctx.should_ignore(9, DisableRule::Untranslated));
        assert!(ctx.should_ignore(10, DisableRule::Untranslated));
        assert!(ctx.should_ignore(15, DisableRule::Untranslated));
        assert!(ctx.should_ignore(20, DisableRule::Untranslated));
        assert!(!ctx.should_ignore(21, DisableRule::Untranslated));

        // Hardcoded is not affected
        assert!(!ctx.should_ignore(15, DisableRule::Hardcoded));
    }

    #[test]
    fn test_disable_context_multiple_ranges() {
        let mut ctx = DisableContext::default();
        ctx.disabled_ranges
            .entry(DisableRule::Hardcoded)
            .or_default()
            .push(DisabledRange { start: 5, end: 10 });
        ctx.disabled_ranges
            .entry(DisableRule::Hardcoded)
            .or_default()
            .push(DisabledRange { start: 20, end: 25 });

        assert!(ctx.should_ignore(7, DisableRule::Hardcoded));
        assert!(!ctx.should_ignore(15, DisableRule::Hardcoded));
        assert!(ctx.should_ignore(22, DisableRule::Hardcoded));
    }
}

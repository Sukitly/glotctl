//! Implementation of suppression directives for glot comments.
//!
//! Supports rule-specific disable comments:
//! - `glot-disable-next-line` - disable all rules for next line
//! - `glot-disable-next-line hardcoded` - disable only hardcoded rule
//! - `glot-disable-next-line untranslated` - disable only untranslated rule
//! - `glot-disable` / `glot-enable` - range-based disabling

use std::collections::{HashMap, HashSet};
use swc_common::{SourceMap, comments::SingleThreadedComments};

use super::super::types::{Directive, DisabledRange, SuppressibleRule, Suppressions};

impl SuppressibleRule {
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
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hardcoded => "hardcoded",
            Self::Untranslated => "untranslated",
        }
    }

    /// Format rules as a sorted, space-separated string for comment suffix.
    ///
    /// Example: `{Hardcoded, Untranslated}` -> `"hardcoded untranslated"`
    pub fn format_rules(rules: &HashSet<Self>) -> String {
        let mut sorted: Vec<_> = rules.iter().collect();
        sorted.sort_by_key(|r| r.as_str());
        sorted
            .iter()
            .map(|r| r.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
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

    /// Parse rule names from the rest of the directive text.
    ///
    /// Design decisions:
    /// - Empty input = all rules (backward compatible with `glot-disable-next-line`)
    /// - Valid tokens only = only those rules (e.g., `hardcoded` = Hardcoded only)
    /// - Mixed valid/invalid = only valid rules (e.g., `hardcoded foobar` = Hardcoded only)
    /// - All invalid tokens = all rules (fail-safe: disable more rather than less)
    ///
    /// The "all invalid = all rules" behavior is intentional for backward compatibility
    /// and fail-safe operation. A typo like `untrasnalted` will disable all rules,
    /// which is more permissive but prevents accidentally leaving issues unchecked.
    fn parse_rules(rest: &str) -> HashSet<SuppressibleRule> {
        let rest = rest.trim();
        if rest.is_empty() {
            // No rules specified = all rules (backward compatible)
            return SuppressibleRule::all();
        }

        let parsed: HashSet<_> = rest
            .split_whitespace()
            .filter_map(SuppressibleRule::parse)
            .collect();

        // If no valid rules parsed, fall back to all rules (fail-safe)
        if parsed.is_empty() {
            SuppressibleRule::all()
        } else {
            parsed
        }
    }
}

impl Suppressions {
    /// Check if a line is suppressed for a specific rule.
    pub fn is_suppressed(&self, line: usize, rule: SuppressibleRule) -> bool {
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

    /// Build Suppressions from SWC comments.
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
        let mut open_ranges: HashMap<SuppressibleRule, usize> = HashMap::new();

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
    // SuppressibleRule Tests
    // ============================================================

    #[test]
    fn test_suppressible_rule_parse_hardcoded() {
        assert_eq!(
            SuppressibleRule::parse("hardcoded"),
            Some(SuppressibleRule::Hardcoded)
        );
        assert_eq!(
            SuppressibleRule::parse("HARDCODED"),
            Some(SuppressibleRule::Hardcoded)
        );
        assert_eq!(
            SuppressibleRule::parse("Hardcoded"),
            Some(SuppressibleRule::Hardcoded)
        );
    }

    #[test]
    fn test_suppressible_rule_parse_untranslated() {
        assert_eq!(
            SuppressibleRule::parse("untranslated"),
            Some(SuppressibleRule::Untranslated)
        );
        assert_eq!(
            SuppressibleRule::parse("UNTRANSLATED"),
            Some(SuppressibleRule::Untranslated)
        );
    }

    #[test]
    fn test_suppressible_rule_parse_unknown() {
        assert_eq!(SuppressibleRule::parse("unknown"), None);
        assert_eq!(SuppressibleRule::parse(""), None);
        assert_eq!(SuppressibleRule::parse("hard-coded"), None);
    }

    #[test]
    fn test_suppressible_rule_all() {
        let all = SuppressibleRule::all();
        assert!(all.contains(&SuppressibleRule::Hardcoded));
        assert!(all.contains(&SuppressibleRule::Untranslated));
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_suppressible_rule_as_str() {
        assert_eq!(SuppressibleRule::Hardcoded.as_str(), "hardcoded");
        assert_eq!(SuppressibleRule::Untranslated.as_str(), "untranslated");
    }

    #[test]
    fn test_format_rules_single() {
        let rules: HashSet<SuppressibleRule> = [SuppressibleRule::Hardcoded].into_iter().collect();
        assert_eq!(SuppressibleRule::format_rules(&rules), "hardcoded");

        let rules: HashSet<SuppressibleRule> =
            [SuppressibleRule::Untranslated].into_iter().collect();
        assert_eq!(SuppressibleRule::format_rules(&rules), "untranslated");
    }

    #[test]
    fn test_format_rules_multiple_sorted() {
        // Should be sorted alphabetically: hardcoded < untranslated
        let rules: HashSet<SuppressibleRule> =
            [SuppressibleRule::Untranslated, SuppressibleRule::Hardcoded]
                .into_iter()
                .collect();
        assert_eq!(
            SuppressibleRule::format_rules(&rules),
            "hardcoded untranslated"
        );
    }

    #[test]
    fn test_format_rules_empty() {
        let rules: HashSet<SuppressibleRule> = HashSet::new();
        assert_eq!(SuppressibleRule::format_rules(&rules), "");
    }

    // ============================================================
    // Directive Tests
    // ============================================================

    #[test]
    fn test_directive_parse_disable_next_line_no_args() {
        let d = Directive::parse("glot-disable-next-line").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(rules.contains(&SuppressibleRule::Untranslated));
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
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(!rules.contains(&SuppressibleRule::Untranslated));
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
                assert!(!rules.contains(&SuppressibleRule::Hardcoded));
                assert!(rules.contains(&SuppressibleRule::Untranslated));
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
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(rules.contains(&SuppressibleRule::Untranslated));
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
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(rules.contains(&SuppressibleRule::Untranslated));
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
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(rules.contains(&SuppressibleRule::Untranslated));
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
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(!rules.contains(&SuppressibleRule::Untranslated));
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
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(rules.contains(&SuppressibleRule::Untranslated));
            }
            _ => panic!("expected Disable"),
        }
    }

    #[test]
    fn test_directive_parse_disable_with_rule() {
        let d = Directive::parse("glot-disable hardcoded").unwrap();
        match d {
            Directive::Disable { rules } => {
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(!rules.contains(&SuppressibleRule::Untranslated));
            }
            _ => panic!("expected Disable"),
        }
    }

    #[test]
    fn test_directive_parse_enable_no_args() {
        let d = Directive::parse("glot-enable").unwrap();
        match d {
            Directive::Enable { rules } => {
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
                assert!(rules.contains(&SuppressibleRule::Untranslated));
            }
            _ => panic!("expected Enable"),
        }
    }

    #[test]
    fn test_directive_parse_enable_with_rule() {
        let d = Directive::parse("glot-enable untranslated").unwrap();
        match d {
            Directive::Enable { rules } => {
                assert!(!rules.contains(&SuppressibleRule::Hardcoded));
                assert!(rules.contains(&SuppressibleRule::Untranslated));
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
                assert!(rules.contains(&SuppressibleRule::Hardcoded));
            }
            _ => panic!("expected DisableNextLine"),
        }
    }

    // ============================================================
    // Suppressions Tests
    // ============================================================

    #[test]
    fn test_suppressions_default() {
        let ctx = Suppressions::default();
        assert!(!ctx.is_suppressed(1, SuppressibleRule::Hardcoded));
        assert!(!ctx.is_suppressed(1, SuppressibleRule::Untranslated));
    }

    #[test]
    fn test_suppressions_single_line() {
        let mut ctx = Suppressions::default();
        ctx.disabled_lines
            .entry(SuppressibleRule::Hardcoded)
            .or_default()
            .insert(5);

        assert!(!ctx.is_suppressed(4, SuppressibleRule::Hardcoded));
        assert!(ctx.is_suppressed(5, SuppressibleRule::Hardcoded));
        assert!(!ctx.is_suppressed(6, SuppressibleRule::Hardcoded));

        // Untranslated is not affected
        assert!(!ctx.is_suppressed(5, SuppressibleRule::Untranslated));
    }

    #[test]
    fn test_suppressions_range() {
        let mut ctx = Suppressions::default();
        ctx.disabled_ranges
            .entry(SuppressibleRule::Untranslated)
            .or_default()
            .push(DisabledRange { start: 10, end: 20 });

        assert!(!ctx.is_suppressed(9, SuppressibleRule::Untranslated));
        assert!(ctx.is_suppressed(10, SuppressibleRule::Untranslated));
        assert!(ctx.is_suppressed(15, SuppressibleRule::Untranslated));
        assert!(ctx.is_suppressed(20, SuppressibleRule::Untranslated));
        assert!(!ctx.is_suppressed(21, SuppressibleRule::Untranslated));

        // Hardcoded is not affected
        assert!(!ctx.is_suppressed(15, SuppressibleRule::Hardcoded));
    }

    #[test]
    fn test_suppressions_multiple_ranges() {
        let mut ctx = Suppressions::default();
        ctx.disabled_ranges
            .entry(SuppressibleRule::Hardcoded)
            .or_default()
            .push(DisabledRange { start: 5, end: 10 });
        ctx.disabled_ranges
            .entry(SuppressibleRule::Hardcoded)
            .or_default()
            .push(DisabledRange { start: 20, end: 25 });

        assert!(ctx.is_suppressed(7, SuppressibleRule::Hardcoded));
        assert!(!ctx.is_suppressed(15, SuppressibleRule::Hardcoded));
        assert!(ctx.is_suppressed(22, SuppressibleRule::Hardcoded));
    }
}

//! Implementation of suppression-related types and methods.
//!
//! Provides:
//! - `SuppressibleRule` - Rules that can be suppressed
//! - `Suppressions` - Query methods for checking suppression status

use std::collections::HashSet;

use crate::core::collect::types::{SuppressibleRule, Suppressions};

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
}

#[cfg(test)]
mod tests {
    use crate::core::collect::comments::suppressions::*;
    use crate::core::collect::types::DisabledRange;

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

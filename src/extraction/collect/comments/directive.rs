//! Glot directive parsing.
//!
//! All glot comment directives:
//! - `glot-disable` / `glot-enable` / `glot-disable-next-line` - suppression
//! - `glot-message-keys` - key declarations

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

use super::super::types::{KeyDeclaration, SuppressibleRule};

static QUOTED_STRING_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());

/// Glot comment directive
#[derive(Debug, Clone)]
pub enum Directive {
    Disable { rules: HashSet<SuppressibleRule> },
    Enable { rules: HashSet<SuppressibleRule> },
    DisableNextLine { rules: HashSet<SuppressibleRule> },
    MessageKeys(KeyDeclaration),
}

impl Directive {
    /// Parse directive from comment text (pure text parsing, no semantic processing)
    ///
    /// # Arguments
    /// * `text` - Comment text (SWC has already stripped `//` and `/* */`)
    ///
    /// # Returns
    /// - `Some(Directive)` if it's a valid glot directive
    /// - `None` if it's not a glot directive or format is invalid
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();

        // Suppression directives (note: check longer prefix first)
        if let Some(rest) = strip_directive_prefix(text, "glot-disable-next-line") {
            return Some(Self::DisableNextLine {
                rules: parse_rules(rest),
            });
        }
        if let Some(rest) = strip_directive_prefix(text, "glot-disable") {
            return Some(Self::Disable {
                rules: parse_rules(rest),
            });
        }
        if let Some(rest) = strip_directive_prefix(text, "glot-enable") {
            return Some(Self::Enable {
                rules: parse_rules(rest),
            });
        }

        // Declaration directive
        if let Some(rest) = strip_directive_prefix(text, "glot-message-keys") {
            return parse_message_keys(rest).map(Self::MessageKeys);
        }

        None
    }
}

/// Try to match directive prefix, requiring whitespace or end after it
fn strip_directive_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.strip_prefix(prefix)
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// Parse suppression rule list
///
/// # Design decisions:
/// - Empty input = all rules (backward compatible)
/// - Valid tokens only = only those rules
/// - Mixed valid/invalid = only valid rules
/// - All invalid tokens = all rules (fail-safe)
fn parse_rules(rest: &str) -> HashSet<SuppressibleRule> {
    let rest = rest.trim();
    if rest.is_empty() {
        return SuppressibleRule::all();
    }

    let parsed: HashSet<_> = rest
        .split_whitespace()
        .filter_map(SuppressibleRule::parse)
        .collect();

    if parsed.is_empty() {
        SuppressibleRule::all()
    } else {
        parsed
    }
}

/// Parse glot-message-keys declaration
///
/// Only performs text parsing, no glob expansion (expansion is deferred until use)
fn parse_message_keys(rest: &str) -> Option<KeyDeclaration> {
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

    // Categorize patterns (but don't expand)
    let mut absolute_patterns = Vec::new();
    let mut relative_patterns = Vec::new();

    for pattern in valid_patterns {
        if pattern.starts_with('.') {
            relative_patterns.push(pattern);
        } else {
            absolute_patterns.push(pattern);
        }
    }

    Some(KeyDeclaration {
        absolute_patterns,
        relative_patterns,
    })
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

    // ============================================================
    // Suppression Directive Tests
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
    fn test_directive_parse_disable_next_line_invalid_rule_falls_back_to_all() {
        let d = Directive::parse("glot-disable-next-line foobar").unwrap();
        match d {
            Directive::DisableNextLine { rules } => {
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
    // Message Keys Directive Tests
    // ============================================================

    #[test]
    fn test_directive_parse_message_keys_single() {
        let d = Directive::parse("glot-message-keys \"Common.submit\"").unwrap();
        match d {
            Directive::MessageKeys(decl) => {
                assert_eq!(decl.absolute_patterns, vec!["Common.submit"]);
                assert!(decl.relative_patterns.is_empty());
            }
            _ => panic!("expected MessageKeys"),
        }
    }

    #[test]
    fn test_directive_parse_message_keys_multiple() {
        let d =
            Directive::parse("glot-message-keys \"Status.active\", \"Status.inactive\"").unwrap();
        match d {
            Directive::MessageKeys(decl) => {
                assert_eq!(
                    decl.absolute_patterns,
                    vec!["Status.active", "Status.inactive"]
                );
                assert!(decl.relative_patterns.is_empty());
            }
            _ => panic!("expected MessageKeys"),
        }
    }

    #[test]
    fn test_directive_parse_message_keys_relative() {
        let d = Directive::parse("glot-message-keys \".submit\"").unwrap();
        match d {
            Directive::MessageKeys(decl) => {
                assert!(decl.absolute_patterns.is_empty());
                assert_eq!(decl.relative_patterns, vec![".submit"]);
            }
            _ => panic!("expected MessageKeys"),
        }
    }

    #[test]
    fn test_directive_parse_message_keys_glob() {
        let d = Directive::parse("glot-message-keys \"errors.*\"").unwrap();
        match d {
            Directive::MessageKeys(decl) => {
                assert_eq!(decl.absolute_patterns, vec!["errors.*"]);
                assert!(decl.relative_patterns.is_empty());
            }
            _ => panic!("expected MessageKeys"),
        }
    }

    #[test]
    fn test_directive_parse_message_keys_mixed() {
        let d = Directive::parse("glot-message-keys \"Common.title\", \".features.*\"").unwrap();
        match d {
            Directive::MessageKeys(decl) => {
                assert_eq!(decl.absolute_patterns, vec!["Common.title"]);
                assert_eq!(decl.relative_patterns, vec![".features.*"]);
            }
            _ => panic!("expected MessageKeys"),
        }
    }

    #[test]
    fn test_directive_parse_message_keys_no_patterns() {
        assert!(Directive::parse("glot-message-keys").is_none());
    }

    #[test]
    fn test_directive_parse_message_keys_prefix_wildcard_invalid() {
        // Prefix wildcard should be filtered out as invalid
        assert!(Directive::parse("glot-message-keys \"*.title\"").is_none());
    }

    #[test]
    fn test_directive_boundary_check() {
        // Should NOT match "glot-disablefoo"
        assert!(Directive::parse("glot-disablefoo").is_none());
        // Should NOT match "glot-disabled"
        assert!(Directive::parse("glot-disabled").is_none());
        // Should match "glot-disable "
        assert!(Directive::parse("glot-disable ").is_some());
    }
}

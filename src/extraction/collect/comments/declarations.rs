//! Implementation of glot-message-keys declarations.
//!
//! Handles parsing of `glot-message-keys` comments that declare expected
//! translation keys for dynamic key expressions:
//! - `// glot-message-keys "Common.key1", "Common.key2"`
//! - `{/* glot-message-keys "Common.*" */}`

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

use super::super::types::{Declarations, KeyDeclaration};
use crate::extraction::extract::TranslationSource;
use crate::extraction::utils::{expand_glob_pattern, is_glob_pattern};

// Matches both JS and JSX comment styles:
// - JS:  // glot-message-keys "key1", "key2"
// - JSX: {/* glot-message-keys "key1", "key2" */}
static GLOT_MESSAGE_KEYS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Capture group 1: content after "glot-message-keys" (patterns)
    // For JSX, stop before */ ; for JS, take rest of line
    Regex::new(r#"(?://\s*glot-message-keys|\{\s*/\*\s*glot-message-keys)\s*(.*?)(?:\s*\*/|\s*$)"#)
        .unwrap()
});

// Matches quoted strings: "some.key"
static QUOTED_STRING_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());

impl Declarations {
    /// Parse all glot-message-keys declarations from source code.
    ///
    /// Handles two types of patterns:
    /// - Absolute patterns: `Namespace.key.path` - expanded immediately
    /// - Relative patterns: `.key.path` (starting with `.`) - stored for later expansion
    ///   with the namespace from the translation function binding
    ///
    /// Invalid patterns are silently skipped.
    pub fn parse(_file_path: &str, source: &str, available_keys: &HashSet<String>) -> Self {
        let mut entries = std::collections::HashMap::new();

        for (line_idx, line) in source.lines().enumerate() {
            let line_num = line_idx + 1;

            if let Some(captures) = GLOT_MESSAGE_KEYS_REGEX.captures(line) {
                let patterns_str = captures.get(1).map_or("", |m| m.as_str());

                let patterns: Vec<String> = QUOTED_STRING_REGEX
                    .captures_iter(patterns_str)
                    .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                    .collect();

                // Skip if no patterns found
                if patterns.is_empty() {
                    continue;
                }

                // Validate patterns and collect valid ones
                let valid_patterns: Vec<String> = patterns
                    .into_iter()
                    .filter(|p| is_valid_pattern(p))
                    .collect();

                if valid_patterns.is_empty() {
                    continue;
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

                // Always create an entry if we had valid patterns,
                // even if they expand to nothing (e.g., glob with no matches)
                entries.insert(
                    line_num,
                    KeyDeclaration {
                        keys: expanded_keys,
                        relative_patterns,
                    },
                );
            }
        }

        Self { entries }
    }

    /// Get declaration for a line. Also checks the previous line
    /// (declaration on the line before the `t()` call).
    pub fn get_declaration(&self, line: usize) -> Option<&KeyDeclaration> {
        // Check current line first
        if let Some(decl) = self.entries.get(&line) {
            return Some(decl);
        }
        // Check previous line (declaration on the line before the t() call)
        if line > 1
            && let Some(decl) = self.entries.get(&(line - 1))
        {
            return Some(decl);
        }
        None
    }

    /// Expand relative patterns (starting with `.`) with namespace(s).
    ///
    /// For Direct sources: expands with the single namespace.
    /// For FromProps sources: expands with all possible namespaces.
    ///
    /// Relative patterns like `.features.*.title` become `Namespace.features.*.title`.
    /// Glob patterns within relative patterns are expanded against available keys.
    /// If a glob pattern doesn't match any keys, the original pattern is preserved
    /// (to allow missing-key detection to report it).
    pub fn expand_relative_patterns(
        relative_patterns: &[String],
        translation_source: &TranslationSource,
        available_keys: &HashSet<String>,
    ) -> Vec<String> {
        if relative_patterns.is_empty() {
            return Vec::new();
        }

        let namespaces = translation_source.namespaces();
        let mut result = Vec::new();

        for pattern in relative_patterns {
            // Remove the leading `.` to get the relative path
            let relative_path = &pattern[1..];

            for namespace in &namespaces {
                let full_pattern = match namespace {
                    Some(ns) => format!("{}.{}", ns, relative_path),
                    None => relative_path.to_string(),
                };

                // If the full pattern contains glob, expand it against available keys
                if is_glob_pattern(&full_pattern) {
                    let expanded = expand_glob_pattern(&full_pattern, available_keys);
                    if expanded.is_empty() {
                        // No matches - keep the pattern for missing-key reporting
                        result.push(full_pattern);
                    } else {
                        result.extend(expanded);
                    }
                } else {
                    result.push(full_pattern);
                }
            }
        }

        result
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

    #[test]
    fn test_parse_empty_source() {
        let store = Declarations::parse("test.tsx", "const x = 1;", &HashSet::new());
        assert!(store.entries.is_empty());
    }

    #[test]
    fn test_parse_absolute_pattern() {
        let source = r#"
// glot-message-keys "Common.submit"
t(`${key}`)
"#;
        let store = Declarations::parse("test.tsx", source, &HashSet::new());

        let decl = store.get_declaration(3);
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
        let store = Declarations::parse("test.tsx", source, &HashSet::new());

        let decl = store.get_declaration(3);
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
        let store = Declarations::parse("test.tsx", source, &available_keys);

        let decl = store.get_declaration(3);
        assert!(decl.is_some());
        let keys = &decl.unwrap().keys;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"Common.btn.submit".to_string()));
        assert!(keys.contains(&"Common.btn.cancel".to_string()));
    }

    #[test]
    fn test_get_declaration_previous_line() {
        let source = r#"
// glot-message-keys "Common.key"
const x = t(`${dynamic}`)
"#;
        let store = Declarations::parse("test.tsx", source, &HashSet::new());

        // Line 3 should find declaration from line 2
        let decl = store.get_declaration(3);
        assert!(decl.is_some());
    }

    #[test]
    fn test_expand_relative_patterns_direct() {
        let patterns = vec![".submit".to_string(), ".cancel".to_string()];
        let source = TranslationSource::Direct {
            namespace: Some("Common".to_string()),
        };

        let expanded = Declarations::expand_relative_patterns(&patterns, &source, &HashSet::new());

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.submit".to_string()));
        assert!(expanded.contains(&"Common.cancel".to_string()));
    }

    #[test]
    fn test_expand_relative_patterns_no_namespace() {
        let patterns = vec![".submit".to_string()];
        let source = TranslationSource::Direct { namespace: None };

        let expanded = Declarations::expand_relative_patterns(&patterns, &source, &HashSet::new());

        assert_eq!(expanded, vec!["submit".to_string()]);
    }

    #[test]
    fn test_expand_relative_patterns_from_props() {
        let patterns = vec![".key".to_string()];
        let source = TranslationSource::FromProps {
            namespaces: vec![Some("Ns1".to_string()), Some("Ns2".to_string())],
        };

        let expanded = Declarations::expand_relative_patterns(&patterns, &source, &HashSet::new());

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Ns1.key".to_string()));
        assert!(expanded.contains(&"Ns2.key".to_string()));
    }

    #[test]
    fn test_expand_relative_patterns_with_glob() {
        let patterns = vec![".btn.*".to_string()];
        let source = TranslationSource::Direct {
            namespace: Some("Common".to_string()),
        };

        let mut available_keys = HashSet::new();
        available_keys.insert("Common.btn.submit".to_string());
        available_keys.insert("Common.btn.cancel".to_string());

        let expanded = Declarations::expand_relative_patterns(&patterns, &source, &available_keys);

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.btn.submit".to_string()));
        assert!(expanded.contains(&"Common.btn.cancel".to_string()));
    }

    // ============================================================
    // Parser tests (from original parser.rs, minus warnings)
    // ============================================================

    #[test]
    fn test_single_key() {
        let source = r#"
// glot-message-keys "Common.submit"
const x = 1;
"#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&2).unwrap();
        assert_eq!(decl.keys, vec!["Common.submit"]);
    }

    #[test]
    fn test_multiple_keys() {
        let source = r#"// glot-message-keys "Status.active", "Status.inactive", "Status.pending""#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(
            decl.keys,
            vec!["Status.active", "Status.inactive", "Status.pending"]
        );
    }

    #[test]
    fn test_glob_pattern() {
        let source = r#"// glot-message-keys "errors.*""#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.keys, Vec::<String>::new()); // Glob with no available keys = empty
    }

    #[test]
    fn test_middle_wildcard() {
        let source = r#"// glot-message-keys "form.*.label""#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.keys, Vec::<String>::new()); // Glob with no available keys = empty
    }

    #[test]
    fn test_prefix_wildcard_skipped() {
        let source = r#"// glot-message-keys "*.title""#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        // Invalid pattern is skipped, no entry created
        assert_eq!(result.entries.len(), 0);
    }

    #[test]
    fn test_multiple_annotations() {
        let source = r#"
// glot-message-keys "errors.*"
function showError() {}

// glot-message-keys "form.*.label", "form.*.placeholder"
function renderForm() {}
"#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 2);
        assert!(result.entries.contains_key(&2));
        assert!(result.entries.contains_key(&5));
    }

    #[test]
    fn test_no_patterns_skipped() {
        let source = r#"// glot-message-keys"#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        // No valid patterns - no entry created
        assert_eq!(result.entries.len(), 0);
    }

    #[test]
    fn test_jsx_comment_single_key() {
        let source = r#"{/* glot-message-keys "Common.submit" */}"#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.keys, vec!["Common.submit"]);
    }

    #[test]
    fn test_jsx_comment_multiple_keys() {
        let source = r#"{/* glot-message-keys "Status.active", "Status.inactive" */}"#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.keys, vec!["Status.active", "Status.inactive"]);
    }

    #[test]
    fn test_jsx_comment_glob_pattern() {
        let source = r#"{/* glot-message-keys "CharacterForm.genderOptions.*" */}"#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.keys, Vec::<String>::new()); // Glob with no available keys = empty
    }

    // ============================================================
    // Relative pattern tests
    // ============================================================

    #[test]
    fn test_relative_pattern_simple() {
        let source = r#"// glot-message-keys ".features.title""#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".features.title"]);
    }

    #[test]
    fn test_relative_pattern_with_glob() {
        let source = r#"// glot-message-keys ".features.*.title""#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".features.*.title"]);
    }

    #[test]
    fn test_relative_pattern_jsx_comment() {
        let source = r#"{/* glot-message-keys ".items.*" */}"#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.relative_patterns, vec![".items.*"]);
    }

    #[test]
    fn test_mixed_absolute_and_relative_patterns() {
        let source = r#"// glot-message-keys "Common.title", ".features.*""#;
        let result = Declarations::parse("test.tsx", source, &HashSet::new());

        assert_eq!(result.entries.len(), 1);
        let decl = result.entries.get(&1).unwrap();
        assert_eq!(decl.keys, vec!["Common.title"]);
        assert_eq!(decl.relative_patterns, vec![".features.*"]);
    }
}

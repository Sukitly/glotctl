//! Implementation of glot-message-keys declarations.
//!
//! Provides query methods for accessing parsed declarations:
//! - `get_declaration`: Look up declaration by line number
//! - `expand_relative_patterns`: Expand relative patterns with namespace

use std::collections::HashSet;

use super::super::types::{Declarations, KeyDeclaration};
use crate::extraction::extract::TranslationSource;
use crate::extraction::utils::{expand_glob_pattern, is_glob_pattern};

impl Declarations {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::collect::comments::collector::CommentCollector;
    use crate::parsers::jsx::parse_jsx_source;

    /// Helper to create Declarations for testing
    fn create_declarations(source: &str, available_keys: &HashSet<String>) -> Declarations {
        let parsed = parse_jsx_source(source.to_string(), "test.tsx").unwrap();
        let comments =
            CommentCollector::collect(&parsed.comments, &parsed.source_map, available_keys);
        comments.declarations
    }

    #[test]
    fn test_get_declaration_previous_line() {
        let source = r#"
// glot-message-keys "Common.key"
const x = t(`${dynamic}`)
"#;
        let store = create_declarations(source, &HashSet::new());

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
}

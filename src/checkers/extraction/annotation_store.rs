//! glot-message-keys annotation parsing and storage.
//!
//! Handles parsing of `glot-message-keys` comments that declare expected
//! translation keys for dynamic key expressions:
//! - `// glot-message-keys "Common.key1", "Common.key2"`
//! - `{/* glot-message-keys "Common.*" */}`

use std::collections::{HashMap, HashSet};

use crate::checkers::glob_matcher::{expand_glob_pattern, is_glob_pattern};
use crate::parsers::comment::{PatternWarning, extract_glot_message_keys};

use super::translation_source::TranslationSource;

/// Annotation data for a single line.
#[derive(Debug, Clone)]
pub struct GlotAnnotation {
    /// Absolute keys after glob expansion (fully qualified keys).
    pub keys: Vec<String>,
    /// Relative patterns (starting with `.`) that need namespace expansion.
    /// e.g., `.features.*.title` will become `Namespace.features.*.title`
    pub relative_patterns: Vec<String>,
}

/// Stores parsed glot-message-keys annotations for a file.
pub struct AnnotationStore {
    /// Line number -> annotation data
    annotations: HashMap<usize, GlotAnnotation>,
    /// Warnings from annotation parsing
    pub warnings: Vec<PatternWarning>,
}

impl AnnotationStore {
    /// Parse all glot-message-keys annotations from source code.
    ///
    /// Handles two types of patterns:
    /// - Absolute patterns: `Namespace.key.path` - expanded immediately
    /// - Relative patterns: `.key.path` (starting with `.`) - stored for later expansion
    ///   with the namespace from the translation function binding
    pub fn parse(source: &str, file_path: &str, available_keys: &HashSet<String>) -> Self {
        let mut annotations = HashMap::new();
        let extract_result = extract_glot_message_keys(source, file_path);

        for annotation in extract_result.annotations {
            let mut expanded_keys = Vec::new();
            let mut relative_patterns = Vec::new();

            for pattern in &annotation.patterns {
                if pattern.starts_with('.') {
                    // Relative pattern - store for later expansion with namespace
                    relative_patterns.push(pattern.clone());
                } else if is_glob_pattern(pattern) {
                    // Absolute glob pattern - expand immediately
                    let expanded = expand_glob_pattern(pattern, available_keys);
                    expanded_keys.extend(expanded);
                } else {
                    // Absolute literal pattern - add as-is
                    expanded_keys.push(pattern.clone());
                }
            }

            if !expanded_keys.is_empty() || !relative_patterns.is_empty() {
                annotations.insert(
                    annotation.line,
                    GlotAnnotation {
                        keys: expanded_keys,
                        relative_patterns,
                    },
                );
            }
        }

        Self {
            annotations,
            warnings: extract_result.warnings,
        }
    }

    /// Get annotation for a line. Also checks the previous line
    /// (annotation on the line before the `t()` call).
    pub fn get_annotation(&self, line: usize) -> Option<&GlotAnnotation> {
        // Check current line first
        if let Some(ann) = self.annotations.get(&line) {
            return Some(ann);
        }
        // Check previous line (annotation on the line before the t() call)
        if line > 1
            && let Some(ann) = self.annotations.get(&(line - 1))
        {
            return Some(ann);
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

    #[test]
    fn test_parse_empty_source() {
        let store = AnnotationStore::parse("const x = 1;", "test.tsx", &HashSet::new());
        assert!(store.annotations.is_empty());
        assert!(store.warnings.is_empty());
    }

    #[test]
    fn test_parse_absolute_pattern() {
        let source = r#"
// glot-message-keys "Common.submit"
t(`${key}`)
"#;
        let store = AnnotationStore::parse(source, "test.tsx", &HashSet::new());

        let ann = store.get_annotation(3);
        assert!(ann.is_some());
        assert_eq!(ann.unwrap().keys, vec!["Common.submit".to_string()]);
        assert!(ann.unwrap().relative_patterns.is_empty());
    }

    #[test]
    fn test_parse_relative_pattern() {
        let source = r#"
// glot-message-keys ".submit"
t(`${key}`)
"#;
        let store = AnnotationStore::parse(source, "test.tsx", &HashSet::new());

        let ann = store.get_annotation(3);
        assert!(ann.is_some());
        assert!(ann.unwrap().keys.is_empty());
        assert_eq!(ann.unwrap().relative_patterns, vec![".submit".to_string()]);
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
        let store = AnnotationStore::parse(source, "test.tsx", &available_keys);

        let ann = store.get_annotation(3);
        assert!(ann.is_some());
        let keys = &ann.unwrap().keys;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"Common.btn.submit".to_string()));
        assert!(keys.contains(&"Common.btn.cancel".to_string()));
    }

    #[test]
    fn test_get_annotation_previous_line() {
        let source = r#"
// glot-message-keys "Common.key"
const x = t(`${dynamic}`)
"#;
        let store = AnnotationStore::parse(source, "test.tsx", &HashSet::new());

        // Line 3 should find annotation from line 2
        let ann = store.get_annotation(3);
        assert!(ann.is_some());
    }

    #[test]
    fn test_expand_relative_patterns_direct() {
        let patterns = vec![".submit".to_string(), ".cancel".to_string()];
        let source = TranslationSource::Direct {
            namespace: Some("Common".to_string()),
        };

        let expanded =
            AnnotationStore::expand_relative_patterns(&patterns, &source, &HashSet::new());

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.submit".to_string()));
        assert!(expanded.contains(&"Common.cancel".to_string()));
    }

    #[test]
    fn test_expand_relative_patterns_no_namespace() {
        let patterns = vec![".submit".to_string()];
        let source = TranslationSource::Direct { namespace: None };

        let expanded =
            AnnotationStore::expand_relative_patterns(&patterns, &source, &HashSet::new());

        assert_eq!(expanded, vec!["submit".to_string()]);
    }

    #[test]
    fn test_expand_relative_patterns_from_props() {
        let patterns = vec![".key".to_string()];
        let source = TranslationSource::FromProps {
            namespaces: vec![Some("Ns1".to_string()), Some("Ns2".to_string())],
        };

        let expanded =
            AnnotationStore::expand_relative_patterns(&patterns, &source, &HashSet::new());

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

        let expanded =
            AnnotationStore::expand_relative_patterns(&patterns, &source, &available_keys);

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.btn.submit".to_string()));
        assert!(expanded.contains(&"Common.btn.cancel".to_string()));
    }
}

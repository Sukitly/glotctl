//! Implementation of glot-message-keys declarations.
//!
//! Provides query and expansion methods for declarations:
//! - `Declarations::get_declaration`: Look up declaration by line number
//! - `KeyDeclaration::expand`: Expand patterns to concrete keys

use std::collections::HashSet;

use crate::core::collect::comments::collector::MAX_COMMENT_CHAIN_LINES;
use crate::core::collect::types::{Declarations, KeyDeclaration};
use crate::core::utils::{expand_glob_pattern, is_glob_pattern};

impl KeyDeclaration {
    /// Expand all patterns to concrete keys.
    ///
    /// This method:
    /// 1. Expands absolute patterns (with glob matching)
    /// 2. Expands relative patterns (with namespace prefix + glob matching)
    ///
    /// # Arguments
    /// * `namespace` - Namespace for expanding relative patterns
    /// * `available_keys` - Available keys for glob matching
    ///
    /// # Returns
    /// A vector of concrete keys. If a glob pattern doesn't match any keys,
    /// the pattern itself is preserved (for missing-key detection).
    #[cfg(test)]
    pub fn expand(&self, namespace: Option<&str>, available_keys: &HashSet<String>) -> Vec<String> {
        let mut result = Vec::new();

        // Expand absolute patterns
        for pattern in &self.absolute_patterns {
            if is_glob_pattern(pattern) {
                let expanded = expand_glob_pattern(pattern, available_keys);
                if expanded.is_empty() {
                    result.push(pattern.clone());
                } else {
                    result.extend(expanded);
                }
            } else {
                result.push(pattern.clone());
            }
        }

        // Expand relative patterns (with namespace prefix)
        for pattern in &self.relative_patterns {
            let relative_path = &pattern[1..]; // Remove leading `.`
            let full_pattern = match namespace {
                Some(ns) => format!("{}.{}", ns, relative_path),
                None => relative_path.to_string(),
            };

            if is_glob_pattern(&full_pattern) {
                let expanded = expand_glob_pattern(&full_pattern, available_keys);
                if expanded.is_empty() {
                    result.push(full_pattern);
                } else {
                    result.extend(expanded);
                }
            } else {
                result.push(full_pattern);
            }
        }

        result
    }

    /// Expand all patterns for multiple namespaces.
    ///
    /// This is more efficient than calling `expand()` multiple times when dealing
    /// with multiple namespaces, as it expands absolute patterns only once.
    ///
    /// # Arguments
    /// * `namespaces` - List of namespaces (for FromProps/FromFnCall sources)
    /// * `available_keys` - Available keys for glob matching
    pub fn expand_all(
        &self,
        namespaces: &[Option<String>],
        available_keys: &HashSet<String>,
    ) -> Vec<String> {
        let mut result = Vec::new();

        // Expand absolute patterns once
        for pattern in &self.absolute_patterns {
            if is_glob_pattern(pattern) {
                let expanded = expand_glob_pattern(pattern, available_keys);
                if expanded.is_empty() {
                    result.push(pattern.clone());
                } else {
                    result.extend(expanded);
                }
            } else {
                result.push(pattern.clone());
            }
        }

        // Expand relative patterns for each namespace
        for pattern in &self.relative_patterns {
            let relative_path = &pattern[1..]; // Remove leading `.`

            for namespace in namespaces {
                let full_pattern = match namespace {
                    Some(ns) => format!("{}.{}", ns, relative_path),
                    None => relative_path.to_string(),
                };

                if is_glob_pattern(&full_pattern) {
                    let expanded = expand_glob_pattern(&full_pattern, available_keys);
                    if expanded.is_empty() {
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

impl Declarations {
    /// Get declaration for a line.
    ///
    /// This method searches backwards from the given line, skipping over
    /// consecutive comment lines to find the nearest declaration. This allows
    /// multiple glot directives to appear on consecutive lines:
    ///
    /// ```tsx
    /// {/* glot-disable-next-line untranslated */}
    /// {/* glot-message-keys "Common.*" */}
    /// {t(`${key}`)}  // <- declaration found from line above
    /// ```
    ///
    /// **Note**: Blank lines break the search chain. The search is also limited
    /// to [`MAX_COMMENT_CHAIN_LINES`] to avoid traversing too far.
    pub fn get_declaration(&self, line: usize) -> Option<&KeyDeclaration> {
        // Check current line first
        if let Some(decl) = self.entries.get(&line) {
            return Some(decl);
        }

        // Search backwards through previous lines, skipping comments
        // Limit to reasonable range to avoid searching too far
        let mut prev = line.saturating_sub(1);
        let min_line = line.saturating_sub(MAX_COMMENT_CHAIN_LINES);

        while prev >= 1 && prev >= min_line {
            // Check if this line has a declaration
            if let Some(decl) = self.entries.get(&prev) {
                return Some(decl);
            }

            // If this line is not a comment, stop searching
            // (declarations must be on immediately preceding comment lines)
            if !self.comment_lines.contains(&prev) {
                break;
            }

            prev = prev.saturating_sub(1);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use crate::core::collect::comments::declarations::*;
    use crate::core::collect::comments::directive::Directive;

    #[test]
    fn test_get_declaration_previous_line() {
        // Create a mock Declarations with entry on line 2
        let mut declarations = Declarations::default();
        let directive = Directive::parse("glot-message-keys \"Common.key\"").unwrap();
        if let Directive::MessageKeys(decl) = directive {
            declarations.entries.insert(2, decl);
        }

        // Line 3 should find declaration from line 2
        let decl = declarations.get_declaration(3);
        assert!(decl.is_some());
    }

    #[test]
    fn test_expand_absolute_patterns() {
        let decl = KeyDeclaration {
            absolute_patterns: vec!["Common.submit".to_string(), "Common.cancel".to_string()],
            relative_patterns: vec![],
        };

        let expanded = decl.expand(None, &HashSet::new());

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.submit".to_string()));
        assert!(expanded.contains(&"Common.cancel".to_string()));
    }

    #[test]
    fn test_expand_relative_patterns_with_namespace() {
        let decl = KeyDeclaration {
            absolute_patterns: vec![],
            relative_patterns: vec![".submit".to_string(), ".cancel".to_string()],
        };

        let expanded = decl.expand(Some("Common"), &HashSet::new());

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.submit".to_string()));
        assert!(expanded.contains(&"Common.cancel".to_string()));
    }

    #[test]
    fn test_expand_relative_patterns_without_namespace() {
        let decl = KeyDeclaration {
            absolute_patterns: vec![],
            relative_patterns: vec![".submit".to_string()],
        };

        let expanded = decl.expand(None, &HashSet::new());

        assert_eq!(expanded, vec!["submit".to_string()]);
    }

    #[test]
    fn test_expand_glob_pattern() {
        let mut available_keys = HashSet::new();
        available_keys.insert("Common.btn.submit".to_string());
        available_keys.insert("Common.btn.cancel".to_string());

        let decl = KeyDeclaration {
            absolute_patterns: vec!["Common.btn.*".to_string()],
            relative_patterns: vec![],
        };

        let expanded = decl.expand(None, &available_keys);

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.btn.submit".to_string()));
        assert!(expanded.contains(&"Common.btn.cancel".to_string()));
    }

    #[test]
    fn test_expand_relative_glob_pattern() {
        let mut available_keys = HashSet::new();
        available_keys.insert("Common.btn.submit".to_string());
        available_keys.insert("Common.btn.cancel".to_string());

        let decl = KeyDeclaration {
            absolute_patterns: vec![],
            relative_patterns: vec![".btn.*".to_string()],
        };

        let expanded = decl.expand(Some("Common"), &available_keys);

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.btn.submit".to_string()));
        assert!(expanded.contains(&"Common.btn.cancel".to_string()));
    }

    #[test]
    fn test_expand_mixed_patterns() {
        let decl = KeyDeclaration {
            absolute_patterns: vec!["Common.title".to_string()],
            relative_patterns: vec![".features.name".to_string()],
        };

        let expanded = decl.expand(Some("App"), &HashSet::new());

        assert_eq!(expanded.len(), 2);
        assert!(expanded.contains(&"Common.title".to_string()));
        assert!(expanded.contains(&"App.features.name".to_string()));
    }
}

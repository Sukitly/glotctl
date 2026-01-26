//! Helper types and functions for rule implementations.
//!
//! This module provides shared utilities used by multiple rules:
//! - `KeyUsageMap`: maps translation keys to their usage locations
//! - Builder functions for these maps

use std::collections::HashMap;

use crate::extraction::{AllKeyUsages, ResolvedKeyUsage};

/// Type alias for key usage map: full_key -> list of resolved key usages.
pub type KeyUsageMap = HashMap<String, Vec<ResolvedKeyUsage>>;

/// Build a map from full_key to all its usage locations across the codebase.
///
/// This is used by replica-lag, untranslated, and type-mismatch rules
/// to show where keys are referenced.
pub fn build_key_usage_map(extractions: &AllKeyUsages) -> KeyUsageMap {
    let mut map: KeyUsageMap = HashMap::new();

    for file_usages in extractions.values() {
        for resolved in &file_usages.resolved {
            map.entry(resolved.key.as_str().to_string())
                .or_default()
                .push(resolved.clone());
        }
    }

    // Sort usages by file path for deterministic output
    for usages in map.values_mut() {
        usages.sort_by(|a, b| {
            a.context
                .location
                .file_path
                .cmp(&b.context.location.file_path)
                .then_with(|| a.context.location.line.cmp(&b.context.location.line))
                .then_with(|| a.context.location.col.cmp(&b.context.location.col))
        });
    }

    map
}

/// Get usages for a key, limited to max_count.
///
/// Returns (usages, total_count) where usages.len() <= max_count.
pub fn get_usages_for_key(key_usages: &KeyUsageMap, key: &str) -> Vec<ResolvedKeyUsage> {
    match key_usages.get(key) {
        Some(usages) => usages.clone(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::analysis::{CommentStyle, SourceContext, SourceLocation};
    use crate::extraction::{FileKeyUsages, FullKey};
    use crate::rules::helpers::*;

    fn make_usage(file: &str, line: usize, col: usize) -> ResolvedKeyUsage {
        ResolvedKeyUsage {
            key: FullKey::new("Common.submit"),
            context: SourceContext::new(
                SourceLocation::new(file, line, col),
                "t('Common.submit')",
                CommentStyle::Js,
            ),
            suppressed_rules: HashSet::new(),
            from_schema: None,
        }
    }

    #[test]
    fn test_build_key_usage_map_empty() {
        let extractions: AllKeyUsages = HashMap::new();
        let map = build_key_usage_map(&extractions);
        assert!(map.is_empty());
    }

    #[test]
    fn test_build_key_usage_map_basic() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![
                    ResolvedKeyUsage {
                        key: FullKey::new("Common.submit"),
                        context: SourceContext::new(
                            SourceLocation::new("test.tsx", 10, 5),
                            "t('Common.submit')",
                            CommentStyle::Jsx,
                        ),
                        suppressed_rules: HashSet::new(),
                        from_schema: None,
                    },
                    ResolvedKeyUsage {
                        key: FullKey::new("Common.submit"),
                        context: SourceContext::new(
                            SourceLocation::new("test.tsx", 20, 5),
                            "t('Common.submit')",
                            CommentStyle::Jsx,
                        ),
                        suppressed_rules: HashSet::new(),
                        from_schema: None,
                    },
                ],
                unresolved: Vec::new(),
            },
        );

        let map = build_key_usage_map(&extractions);
        let usages = map.get("Common.submit").unwrap();
        assert_eq!(usages.len(), 2);
        // Should be sorted by line
        assert_eq!(usages[0].context.location.line, 10);
        assert_eq!(usages[1].context.location.line, 20);
    }

    #[test]
    fn test_get_usages_for_key_found() {
        let mut map: KeyUsageMap = HashMap::new();
        map.insert(
            "Common.submit".to_string(),
            vec![
                make_usage("a.tsx", 1, 1),
                make_usage("b.tsx", 2, 1),
                make_usage("c.tsx", 3, 1),
            ],
        );

        let usages = get_usages_for_key(&map, "Common.submit");
        assert_eq!(usages.len(), 3);
    }

    #[test]
    fn test_get_usages_for_key_not_found() {
        let map: KeyUsageMap = HashMap::new();
        let usages = get_usages_for_key(&map, "Common.submit");
        assert!(usages.is_empty());
    }
}

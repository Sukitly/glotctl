//! Helper types and functions for rule implementations.
//!
//! This module provides shared utilities used by multiple rules:
//! - `KeyUsageMap`: maps translation keys to their usage locations
//! - `KeyDisableMap`: tracks which keys have suppressed rules
//! - Builder functions for these maps

use std::collections::HashMap;

use crate::{
    extraction::collect::SuppressibleRule,
    types::{context::SourceContext, key_usage::AllKeyUsages},
};

/// Maximum number of usage locations to include in issues.
pub const MAX_KEY_USAGES: usize = 3;

/// Type alias for key usage map: full_key -> list of usage locations.
pub type KeyUsageMap = HashMap<String, Vec<SourceContext>>;

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
                .push(resolved.context.clone());
        }
    }

    // Sort usages by file path for deterministic output
    for usages in map.values_mut() {
        usages.sort_by(|a, b| {
            a.location
                .file_path
                .cmp(&b.location.file_path)
                .then_with(|| a.location.line.cmp(&b.location.line))
                .then_with(|| a.location.col.cmp(&b.location.col))
        });
    }

    map
}

/// Get usages for a key, limited to max_count.
///
/// Returns (usages, total_count) where usages.len() <= max_count.
pub fn get_usages_for_key(
    key_usages: &KeyUsageMap,
    key: &str,
    max_count: usize,
) -> (Vec<SourceContext>, usize) {
    match key_usages.get(key) {
        Some(usages) => {
            let total = usages.len();
            let limited: Vec<SourceContext> = usages.iter().take(max_count).cloned().collect();
            (limited, total)
        }
        None => (Vec::new(), 0),
    }
}

/// Stats about disable comments for a key's usages.
#[derive(Debug, Default, Clone)]
pub struct KeyDisableStats {
    /// Total number of usages for this key.
    pub total_usages: usize,
    /// Number of usages with untranslated rule disabled.
    pub disabled_usages: usize,
}

impl KeyDisableStats {
    /// Returns true if all usages have the untranslated rule disabled.
    pub fn all_disabled(&self) -> bool {
        self.total_usages > 0 && self.disabled_usages == self.total_usages
    }
}

/// Type alias for key disable stats map: full_key -> disable stats.
pub type KeyDisableMap = HashMap<String, KeyDisableStats>;

/// Build a map of disable stats for untranslated rule from extractions.
///
/// For each key, tracks how many usages have `glot-disable-next-line untranslated`.
/// A key is fully disabled if ALL its usages have the disable comment.
pub fn build_key_disable_map(extractions: &AllKeyUsages) -> KeyDisableMap {
    let mut map: KeyDisableMap = HashMap::new();

    for file_usages in extractions.values() {
        for resolved in &file_usages.resolved {
            let stats = map.entry(resolved.key.as_str().to_string()).or_default();
            stats.total_usages += 1;
            if resolved
                .suppressed_rules
                .contains(&SuppressibleRule::Untranslated)
            {
                stats.disabled_usages += 1;
            }
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::types::{
        context::{CommentStyle, SourceLocation},
        key_usage::{FileKeyUsages, FullKey, ResolvedKeyUsage},
    };

    #[test]
    fn test_key_disable_stats_all_disabled_true() {
        let stats = KeyDisableStats {
            total_usages: 3,
            disabled_usages: 3,
        };
        assert!(stats.all_disabled());
    }

    #[test]
    fn test_key_disable_stats_all_disabled_false_some_not_disabled() {
        let stats = KeyDisableStats {
            total_usages: 3,
            disabled_usages: 2,
        };
        assert!(!stats.all_disabled());
    }

    #[test]
    fn test_key_disable_stats_all_disabled_false_no_usages() {
        let stats = KeyDisableStats {
            total_usages: 0,
            disabled_usages: 0,
        };
        assert!(!stats.all_disabled());
    }

    #[test]
    fn test_key_disable_stats_default() {
        let stats = KeyDisableStats::default();
        assert_eq!(stats.total_usages, 0);
        assert_eq!(stats.disabled_usages, 0);
        assert!(!stats.all_disabled());
    }

    #[test]
    fn test_build_key_disable_map_empty() {
        let extractions: AllKeyUsages = HashMap::new();
        let map = build_key_disable_map(&extractions);
        assert!(map.is_empty());
    }

    #[test]
    fn test_build_key_disable_map_basic() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![
                    ResolvedKeyUsage {
                        key: FullKey::new("Common.submit"),
                        context: SourceContext::new(
                            SourceLocation::new("test.tsx", 1, 1),
                            "t('Common.submit')",
                            CommentStyle::Js,
                        ),
                        suppressed_rules: HashSet::new(),
                        from_schema: None,
                    },
                    ResolvedKeyUsage {
                        key: FullKey::new("Common.submit"),
                        context: SourceContext::new(
                            SourceLocation::new("test.tsx", 2, 1),
                            "t('Common.submit')",
                            CommentStyle::Js,
                        ),
                        suppressed_rules: [SuppressibleRule::Untranslated].into_iter().collect(),
                        from_schema: None,
                    },
                ],
                unresolved: Vec::new(),
            },
        );

        let map = build_key_disable_map(&extractions);
        let stats = map.get("Common.submit").unwrap();
        assert_eq!(stats.total_usages, 2);
        assert_eq!(stats.disabled_usages, 1);
        assert!(!stats.all_disabled());
    }

    #[test]
    fn test_build_key_disable_map_all_disabled() {
        let mut extractions: AllKeyUsages = HashMap::new();
        extractions.insert(
            "test.tsx".to_string(),
            FileKeyUsages {
                resolved: vec![
                    ResolvedKeyUsage {
                        key: FullKey::new("Common.submit"),
                        context: SourceContext::new(
                            SourceLocation::new("test.tsx", 1, 1),
                            "t('Common.submit')",
                            CommentStyle::Js,
                        ),
                        suppressed_rules: [SuppressibleRule::Untranslated].into_iter().collect(),
                        from_schema: None,
                    },
                    ResolvedKeyUsage {
                        key: FullKey::new("Common.submit"),
                        context: SourceContext::new(
                            SourceLocation::new("test.tsx", 2, 1),
                            "t('Common.submit')",
                            CommentStyle::Js,
                        ),
                        suppressed_rules: [SuppressibleRule::Untranslated].into_iter().collect(),
                        from_schema: None,
                    },
                ],
                unresolved: Vec::new(),
            },
        );

        let map = build_key_disable_map(&extractions);
        let stats = map.get("Common.submit").unwrap();
        assert_eq!(stats.total_usages, 2);
        assert_eq!(stats.disabled_usages, 2);
        assert!(stats.all_disabled());
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
        assert_eq!(usages[0].location.line, 10);
        assert_eq!(usages[1].location.line, 20);
    }

    #[test]
    fn test_get_usages_for_key_found() {
        let mut map: KeyUsageMap = HashMap::new();
        map.insert(
            "Common.submit".to_string(),
            vec![
                SourceContext::new(
                    SourceLocation::new("a.tsx", 1, 1),
                    "t('Common.submit')",
                    CommentStyle::Js,
                ),
                SourceContext::new(
                    SourceLocation::new("b.tsx", 2, 1),
                    "t('Common.submit')",
                    CommentStyle::Js,
                ),
                SourceContext::new(
                    SourceLocation::new("c.tsx", 3, 1),
                    "t('Common.submit')",
                    CommentStyle::Js,
                ),
            ],
        );

        let (usages, total) = get_usages_for_key(&map, "Common.submit", 2);
        assert_eq!(total, 3);
        assert_eq!(usages.len(), 2);
    }

    #[test]
    fn test_get_usages_for_key_not_found() {
        let map: KeyUsageMap = HashMap::new();
        let (usages, total) = get_usages_for_key(&map, "Common.submit", 2);
        assert_eq!(total, 0);
        assert!(usages.is_empty());
    }
}

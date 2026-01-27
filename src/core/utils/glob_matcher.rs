use std::collections::HashSet;

/// Check if a pattern contains wildcards.
pub fn is_glob_pattern(pattern: &str) -> bool {
    pattern.contains('*')
}

/// Expand a glob pattern against a set of available keys.
///
/// The `*` wildcard matches exactly one segment (doesn't cross `.` boundaries).
///
/// Examples:
/// - `errors.*` matches `errors.E001`, `errors.E002` but not `errors.network.timeout`
/// - `form.*.label` matches `form.email.label`, `form.password.label`
pub fn expand_glob_pattern(pattern: &str, available_keys: &HashSet<String>) -> Vec<String> {
    if !is_glob_pattern(pattern) {
        // Literal pattern - return as-is if it exists in available_keys
        if available_keys.contains(pattern) {
            return vec![pattern.to_string()];
        }
        return vec![];
    }

    let pattern_segments: Vec<&str> = pattern.split('.').collect();

    available_keys
        .iter()
        .filter(|key| matches_pattern(&pattern_segments, key))
        .cloned()
        .collect()
}

/// Check if a key matches a pattern.
fn matches_pattern(pattern_segments: &[&str], key: &str) -> bool {
    let key_segments: Vec<&str> = key.split('.').collect();

    // Must have same number of segments
    if pattern_segments.len() != key_segments.len() {
        return false;
    }

    // Each segment must match
    pattern_segments
        .iter()
        .zip(key_segments.iter())
        .all(|(pattern_seg, key_seg)| segment_matches(pattern_seg, key_seg))
}

/// Match a single segment with potential wildcards.
/// `*` matches 0 or more characters (standard glob semantics).
fn segment_matches(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true; // Matches any segment
    }
    if !pattern.contains('*') {
        return pattern == text; // Exact match
    }

    // Partial match with wildcards
    let parts: Vec<&str> = pattern.split('*').collect();

    let mut pos = 0;

    // First part must be a prefix
    if !parts[0].is_empty() {
        if !text.starts_with(parts[0]) {
            return false;
        }
        pos = parts[0].len();
    }

    // Last part must be a suffix
    let last = parts[parts.len() - 1];
    let end = if last.is_empty() {
        text.len()
    } else {
        if !text.ends_with(last) {
            return false;
        }
        // Check for overlap between prefix and suffix
        if pos + last.len() > text.len() {
            return false;
        }
        text.len() - last.len()
    };

    // Middle parts must appear in order
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue; // ** means match anything
        }
        if let Some(found) = text[pos..end].find(part) {
            pos = pos + found + part.len();
        } else {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use crate::core::utils::glob_matcher::*;

    fn make_keys(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_is_glob_pattern() {
        assert!(!is_glob_pattern("Common.submit"));
        assert!(is_glob_pattern("errors.*"));
        assert!(is_glob_pattern("form.*.label"));
    }

    #[test]
    fn test_literal_pattern_exists() {
        let keys = make_keys(&["Common.submit", "Common.cancel"]);
        let result = expand_glob_pattern("Common.submit", &keys);

        assert_eq!(result, vec!["Common.submit"]);
    }

    #[test]
    fn test_literal_pattern_not_exists() {
        let keys = make_keys(&["Common.submit", "Common.cancel"]);
        let result = expand_glob_pattern("Common.delete", &keys);

        assert!(result.is_empty());
    }

    #[test]
    fn test_suffix_wildcard() {
        let keys = make_keys(&[
            "errors.E001",
            "errors.E002",
            "errors.E003",
            "auth.login",
            "auth.logout",
        ]);
        let mut result = expand_glob_pattern("errors.*", &keys);
        result.sort();

        assert_eq!(result, vec!["errors.E001", "errors.E002", "errors.E003"]);
    }

    #[test]
    fn test_middle_wildcard() {
        let keys = make_keys(&[
            "form.email.label",
            "form.email.placeholder",
            "form.password.label",
            "form.password.placeholder",
            "form.submit",
        ]);
        let mut result = expand_glob_pattern("form.*.label", &keys);
        result.sort();

        assert_eq!(result, vec!["form.email.label", "form.password.label"]);
    }

    #[test]
    fn test_wildcard_does_not_match_nested() {
        let keys = make_keys(&[
            "errors.network.timeout",
            "errors.network.connection",
            "errors.E001",
        ]);
        let mut result = expand_glob_pattern("errors.*", &keys);
        result.sort();

        // Only matches single segment after "errors."
        assert_eq!(result, vec!["errors.E001"]);
    }

    #[test]
    fn test_multiple_wildcards() {
        let keys = make_keys(&[
            "form.email.label.text",
            "form.password.hint.text",
            "other.field.label.text",
        ]);
        let mut result = expand_glob_pattern("form.*.*.text", &keys);
        result.sort();

        assert_eq!(
            result,
            vec!["form.email.label.text", "form.password.hint.text"]
        );
    }

    #[test]
    fn test_no_matches() {
        let keys = make_keys(&["auth.login", "auth.logout"]);
        let result = expand_glob_pattern("errors.*", &keys);

        assert!(result.is_empty());
    }

    // Tests for partial segment matching (new feature)

    #[test]
    fn test_partial_segment_prefix_wildcard() {
        let keys = make_keys(&[
            "step1.title",
            "step2.title",
            "step5.title",
            "step.title",
            "other.title",
        ]);
        let mut result = expand_glob_pattern("step*.title", &keys);
        result.sort();

        // * matches 0 or more characters
        assert_eq!(
            result,
            vec!["step.title", "step1.title", "step2.title", "step5.title"]
        );
    }

    #[test]
    fn test_partial_segment_suffix_wildcard() {
        let keys = make_keys(&[
            "genderOptions.male",
            "genderOptions.female",
            "Options.other",
            "colorOptions.red",
        ]);
        let mut result = expand_glob_pattern("*Options.*", &keys);
        result.sort();

        assert_eq!(
            result,
            vec![
                "Options.other",
                "colorOptions.red",
                "genderOptions.female",
                "genderOptions.male"
            ]
        );
    }

    #[test]
    fn test_partial_segment_both_sides() {
        let keys = make_keys(&["prefixMiddleSuffix.key", "prefixSuffix.key", "other.key"]);
        let mut result = expand_glob_pattern("prefix*Suffix.*", &keys);
        result.sort();

        assert_eq!(result, vec!["prefixMiddleSuffix.key", "prefixSuffix.key"]);
    }

    #[test]
    fn test_segment_matches_function() {
        // Direct tests for segment_matches
        assert!(segment_matches("*", "anything"));
        assert!(segment_matches("exact", "exact"));
        assert!(!segment_matches("exact", "different"));

        // Prefix wildcard
        assert!(segment_matches("step*", "step1"));
        assert!(segment_matches("step*", "step"));
        assert!(segment_matches("step*", "stepFoo"));
        assert!(!segment_matches("step*", "Step1")); // case sensitive

        // Suffix wildcard
        assert!(segment_matches("*Options", "genderOptions"));
        assert!(segment_matches("*Options", "Options"));
        assert!(!segment_matches("*Options", "genderoptions")); // case sensitive

        // Both sides
        assert!(segment_matches("pre*suf", "preMIDsuf"));
        assert!(segment_matches("pre*suf", "presuf"));
        assert!(!segment_matches("pre*suf", "presufX"));
        assert!(!segment_matches("pre*suf", "Xpresuf"));
    }
}

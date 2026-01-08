use regex::Regex;
use std::sync::LazyLock;

/// Represents a glot-message-keys annotation found in source code.
#[derive(Debug, Clone)]
pub struct GlotMessageKeys {
    /// The patterns declared in the annotation (may contain `*` wildcards).
    pub patterns: Vec<String>,
    /// Line number where the annotation was found.
    pub line: usize,
}

/// Warning for invalid glot-message-keys patterns.
#[derive(Debug, Clone, Default)]
pub struct PatternWarning {
    pub message: String,
    pub file_path: String,
    pub line: usize,
}

/// Result of extracting glot-message-keys from source code.
#[derive(Debug, Default)]
pub struct ExtractResult {
    pub annotations: Vec<GlotMessageKeys>,
    pub warnings: Vec<PatternWarning>,
}

// Matches both JS and JSX comment styles:
// - JS:  // glot-message-keys "key1", "key2"
// - JSX: {/* glot-message-keys "key1", "key2" */}
// Also matches without patterns to emit warning.
static GLOT_MESSAGE_KEYS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Capture group 1: content after "glot-message-keys" (patterns)
    // For JSX, stop before */ ; for JS, take rest of line
    Regex::new(r#"(?://\s*glot-message-keys|\{\s*/\*\s*glot-message-keys)\s*(.*?)(?:\s*\*/|\s*$)"#)
        .unwrap()
});

// Matches quoted strings: "some.key"
static QUOTED_STRING_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());

/// Extract all glot-message-keys annotations from source code.
pub fn extract_glot_message_keys(source: &str, file_path: &str) -> ExtractResult {
    let mut result = ExtractResult::default();

    for (line_idx, line) in source.lines().enumerate() {
        let line_num = line_idx + 1;

        if let Some(captures) = GLOT_MESSAGE_KEYS_REGEX.captures(line) {
            let patterns_str = captures.get(1).map_or("", |m| m.as_str());

            let patterns: Vec<String> = QUOTED_STRING_REGEX
                .captures_iter(patterns_str)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .collect();

            if patterns.is_empty() {
                result.warnings.push(PatternWarning {
                    message: "glot-message-keys annotation has no valid patterns".to_string(),
                    file_path: file_path.to_string(),
                    line: line_num,
                });
                continue;
            }

            // Validate patterns
            let mut valid_patterns = Vec::new();
            for pattern in &patterns {
                if let Some(warning) = validate_pattern(pattern) {
                    result.warnings.push(PatternWarning {
                        message: warning,
                        file_path: file_path.to_string(),
                        line: line_num,
                    });
                } else {
                    valid_patterns.push(pattern.clone());
                }
            }

            if !valid_patterns.is_empty() {
                result.annotations.push(GlotMessageKeys {
                    patterns: valid_patterns,
                    line: line_num,
                });
            }
        }
    }

    result
}

/// Validate a pattern. Returns Some(warning_message) if invalid.
fn validate_pattern(pattern: &str) -> Option<String> {
    let segments: Vec<&str> = pattern.split('.').collect();

    // Check for prefix wildcard pattern like "*.suffix"
    if let Some(first) = segments.first()
        && *first == "*"
        && segments.len() > 1
    {
        return Some(format!(
            "Prefix wildcard pattern '{}' is not supported. Consider restructuring your code to use a fixed namespace.",
            pattern
        ));
    }

    // Check for empty segments
    if segments.iter().any(|s| s.is_empty()) {
        return Some(format!(
            "Invalid pattern '{}': contains empty segment",
            pattern
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_key() {
        let source = r#"
// glot-message-keys "Common.submit"
const x = 1;
"#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 1);
        assert_eq!(result.annotations[0].patterns, vec!["Common.submit"]);
        assert_eq!(result.annotations[0].line, 2);
    }

    #[test]
    fn test_multiple_keys() {
        let source = r#"// glot-message-keys "Status.active", "Status.inactive", "Status.pending""#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 1);
        assert_eq!(
            result.annotations[0].patterns,
            vec!["Status.active", "Status.inactive", "Status.pending"]
        );
    }

    #[test]
    fn test_glob_pattern() {
        let source = r#"// glot-message-keys "errors.*""#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 1);
        assert_eq!(result.annotations[0].patterns, vec!["errors.*"]);
    }

    #[test]
    fn test_middle_wildcard() {
        let source = r#"// glot-message-keys "form.*.label""#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 1);
        assert_eq!(result.annotations[0].patterns, vec!["form.*.label"]);
    }

    #[test]
    fn test_prefix_wildcard_warning() {
        let source = r#"// glot-message-keys "*.title""#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 0);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("Prefix wildcard"));
    }

    #[test]
    fn test_multiple_annotations() {
        let source = r#"
// glot-message-keys "errors.*"
function showError() {}

// glot-message-keys "form.*.label", "form.*.placeholder"
function renderForm() {}
"#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 2);
        assert_eq!(result.annotations[0].patterns, vec!["errors.*"]);
        assert_eq!(
            result.annotations[1].patterns,
            vec!["form.*.label", "form.*.placeholder"]
        );
    }

    #[test]
    fn test_no_patterns_warning() {
        let source = r#"// glot-message-keys"#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 0);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("no valid patterns"));
    }

    #[test]
    fn test_jsx_comment_single_key() {
        let source = r#"{/* glot-message-keys "Common.submit" */}"#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 1);
        assert_eq!(result.annotations[0].patterns, vec!["Common.submit"]);
    }

    #[test]
    fn test_jsx_comment_multiple_keys() {
        let source = r#"{/* glot-message-keys "Status.active", "Status.inactive" */}"#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 1);
        assert_eq!(
            result.annotations[0].patterns,
            vec!["Status.active", "Status.inactive"]
        );
    }

    #[test]
    fn test_jsx_comment_glob_pattern() {
        let source = r#"{/* glot-message-keys "CharacterForm.genderOptions.*" */}"#;
        let result = extract_glot_message_keys(source, "test.tsx");

        assert_eq!(result.annotations.len(), 1);
        assert_eq!(
            result.annotations[0].patterns,
            vec!["CharacterForm.genderOptions.*"]
        );
    }
}

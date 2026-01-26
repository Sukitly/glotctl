//! Common utility functions shared across the codebase.

/// Checks if the text contains at least one Unicode alphabetic character.
///
/// Returns false for empty strings, pure numbers, or pure symbols.
///
/// # Examples
///
/// ```
/// use glot::utils::contains_alphabetic;
///
/// assert!(contains_alphabetic("Hello"));
/// assert!(contains_alphabetic("你好"));
/// assert!(contains_alphabetic("Hello123"));
/// assert!(!contains_alphabetic("123"));
/// assert!(!contains_alphabetic("---"));
/// assert!(!contains_alphabetic("$100"));
/// assert!(!contains_alphabetic(""));
/// ```
pub fn contains_alphabetic(text: &str) -> bool {
    text.chars().any(|c| c.is_alphabetic())
}

#[cfg(test)]
mod tests {
    use crate::utils::*;

    #[test]
    fn test_contains_alphabetic() {
        // Should return true for text with letters
        assert!(contains_alphabetic("Hello"));
        assert!(contains_alphabetic("你好"));
        assert!(contains_alphabetic("Hello123"));
        assert!(contains_alphabetic("123 abc"));
        assert!(contains_alphabetic("  abc  "));
        assert!(contains_alphabetic("Test!@#"));

        // Should return false for text without letters
        assert!(!contains_alphabetic("123"));
        assert!(!contains_alphabetic("---"));
        assert!(!contains_alphabetic("$100"));
        assert!(!contains_alphabetic("!@#$%"));
        assert!(!contains_alphabetic("   "));
        assert!(!contains_alphabetic(""));
        assert!(!contains_alphabetic("123-456"));
    }
}

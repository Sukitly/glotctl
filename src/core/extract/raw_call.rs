//! Raw translation call data collected during Phase 2: Extraction.
//!
//! This module defines the intermediate representation for translation calls
//! collected during AST traversal. These "raw" calls contain unresolved keys
//! that will be validated against locale files in Phase 3: Resolution.
//!
//! The resolution process in Phase 3 produces either:
//! - `ResolvedKeyUsage` (valid keys found in locale files)
//! - `UnresolvedKeyUsage` (keys not found, dynamic keys, or unresolvable expressions)

use crate::core::SourceContext;
use crate::core::extract::{TranslationSource, ValueSource};

/// Translation function call kind.
///
/// Distinguishes between direct translation calls and method calls that
/// affect value type resolution in Phase 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationCallKind {
    /// Direct call: `t("key")` or `t(\`key.\${suffix}\`)`
    ///
    /// The value type (string vs array) is determined from the locale file.
    Direct,

    /// Method call: `t.raw("key")`, `t.rich("key")`, `t.markup("key")`
    ///
    /// These methods explicitly specify the expected value type:
    /// - `raw()`: Expects a plain string value (not a rich text object)
    /// - `rich()`: Expects a rich text object with formatting
    /// - `markup()`: Expects markup/HTML string
    ///
    /// The string is the method name (e.g., "raw", "rich", "markup").
    Method(String),
}

/// Raw translation call data collected during Phase 2: Extraction.
///
/// This is the handoff point between Phase 2 (Extraction) and Phase 3 (Resolution).
/// Each `RawTranslationCall` represents a single translation function call found
/// in source code, with its key argument analyzed but not yet validated.
///
/// # Lifecycle
///
/// 1. **Phase 2 (Extraction)**: Created by `FileAnalyzer` during AST traversal
/// 2. **Phase 3 (Resolution)**: Processed by `crate::core::resolve::resolve_translation_call`
///    to produce `ResolvedKeyUsage` or `UnresolvedKeyUsage`
///
/// # Examples
///
/// ```ignore
/// // Direct call with literal key → argument is ValueSource::Resolved("home.title")
/// t("home.title")
///
/// // Direct call with template → argument is ValueSource::PartiallyResolved(...)
/// t(`home.${section}`)
///
/// // Method call → call_kind is Method("raw")
/// t.raw("common.button.submit")
///
/// // Call from props → translation_source is TranslationSource::FromProps
/// function MyComponent({ t }: Props) {
///   return t("key"); // t comes from props
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RawTranslationCall {
    /// Source code context (file path, line, column, source line, comment style).
    ///
    /// Used for error reporting and suppression checking in Phase 3.
    pub context: SourceContext,

    /// Where the translation function came from.
    ///
    /// This determines which namespaces to search for the key:
    /// - `Direct { namespace }`: From `useTranslations("namespace")`
    /// - `FromProps { namespaces }`: From component props (may have multiple possible namespaces)
    /// - `FromFnCall { namespaces }`: From function call parameter
    /// - `Shadowed`: Variable shadowed a translation binding (won't be resolved)
    pub translation_source: TranslationSource,

    /// The translation key argument, analyzed from the AST expression.
    ///
    /// This is the result of analyzing the first argument to the translation call:
    /// - `Resolved("key")`: Literal string key
    /// - `PartiallyResolved(...)`: Template string with known prefix/suffix
    /// - `Unresolvable(...)`: Complex expression we can't statically analyze
    ///
    /// Phase 3 uses this to determine if the key exists in locale files.
    pub argument: ValueSource,

    /// Whether this is a direct call or method call.
    ///
    /// Affects how Phase 3 validates the key's value type:
    /// - `Direct`: Accept any value type from locale file
    /// - `Method("raw")`: Expect string value (not rich text object)
    pub call_kind: TranslationCallKind,
}

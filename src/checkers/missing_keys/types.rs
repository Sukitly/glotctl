//! Type definitions for missing key detection.

use crate::checkers::schema::SchemaCallInfo;
use crate::checkers::value_source::ResolvedKey;
use crate::parsers::comment::PatternWarning;

/// Result of scanning a file for missing translation keys.
#[derive(Debug, Default)]
pub struct MissingKeyResult {
    pub used_keys: Vec<UsedKey>,
    pub warnings: Vec<DynamicKeyWarning>,
    pub schema_calls: Vec<SchemaCallInfo>,
    /// Resolved keys from ValueAnalyzer
    pub resolved_keys: Vec<ResolvedKey>,
    /// Warnings from glot-message-keys annotation parsing
    pub pattern_warnings: Vec<PatternWarning>,
}

/// A translation key used in code.
#[derive(Debug, Clone)]
pub struct UsedKey {
    pub full_key: String,
    pub file_path: String,
    pub line: usize,
    pub col: usize,
    pub source_line: String,
}

/// Reason why a key is considered dynamic.
#[derive(Debug, Clone)]
pub enum DynamicKeyReason {
    /// Key is a variable: t(keyName)
    VariableKey,
    /// Key is a template with expressions: t(`${prefix}.key`)
    TemplateWithExpr,
}

/// Warning about a dynamic key that cannot be statically analyzed.
#[derive(Debug, Clone)]
pub struct DynamicKeyWarning {
    pub file_path: String,
    pub line: usize,
    pub col: usize,
    pub reason: DynamicKeyReason,
    pub source_line: String,
    /// Suggested pattern hint for the user (formatted message)
    pub hint: Option<String>,
    /// The raw pattern inferred from template (e.g., "Common.*.submit")
    /// Used by fix command to generate glot-message-keys comments
    pub pattern: Option<String>,
    /// Whether this warning is in JSX context (affects comment syntax)
    pub in_jsx_context: bool,
}

/// Stores glot-message-keys annotation data for a line.
#[derive(Debug, Clone)]
pub(super) struct GlotAnnotation {
    /// Absolute keys after glob expansion (fully qualified keys).
    pub keys: Vec<String>,
    /// Relative patterns (starting with `.`) that need namespace expansion.
    /// e.g., `.features.*.title` will become `Namespace.features.*.title`
    pub relative_patterns: Vec<String>,
}

/// Source of a translation function binding.
///
/// Distinguishes between translation functions obtained directly via
/// `useTranslations()`/`getTranslations()` vs those passed as props or function arguments.
#[derive(Debug, Clone)]
pub enum TranslationSource {
    /// Direct binding: `const t = useTranslations("Namespace")`
    /// Has a single, known namespace.
    Direct { namespace: Option<String> },
    /// From props: `function Component({ t }: Props)`
    /// May have multiple possible namespaces from different call sites.
    FromProps {
        /// All possible namespaces from call sites.
        /// Empty if no call sites found (will still generate warnings).
        namespaces: Vec<Option<String>>,
    },
    /// From function call argument: `const usageLabels = (t) => { ... }; usageLabels(t)`
    /// Similar to FromProps, may have multiple possible namespaces from different call sites.
    FromFnCall {
        /// All possible namespaces from call sites.
        /// Empty if no call sites found (will still generate warnings).
        namespaces: Vec<Option<String>>,
    },
    /// Shadowed binding: a parameter that shadows an outer translation binding.
    /// Used when an inner function has a parameter with the same name as an outer
    /// translation binding, but the inner function is not tracked.
    /// Calls using this binding should NOT be tracked.
    Shadowed,
}

impl TranslationSource {
    /// Returns true if this is a shadowed binding (should not be tracked).
    pub fn is_shadowed(&self) -> bool {
        matches!(self, TranslationSource::Shadowed)
    }

    /// Returns true if this is an indirect source (props or function call).
    pub fn is_indirect(&self) -> bool {
        matches!(
            self,
            TranslationSource::FromProps { .. } | TranslationSource::FromFnCall { .. }
        )
    }

    /// Get all possible namespaces.
    /// For Direct, returns a single-element vector.
    /// For FromProps/FromFnCall, returns all namespaces from call sites.
    /// For Shadowed, returns empty (should not be called, but safe fallback).
    pub fn namespaces(&self) -> Vec<Option<String>> {
        match self {
            TranslationSource::Direct { namespace } => vec![namespace.clone()],
            TranslationSource::FromProps { namespaces }
            | TranslationSource::FromFnCall { namespaces } => namespaces.clone(),
            TranslationSource::Shadowed => vec![],
        }
    }

    /// Get the primary namespace (for backward compatibility).
    /// For Direct, returns the namespace.
    /// For FromProps/FromFnCall/Shadowed, returns None (namespace is dynamic or not applicable).
    pub fn primary_namespace(&self) -> Option<String> {
        match self {
            TranslationSource::Direct { namespace } => namespace.clone(),
            TranslationSource::FromProps { .. }
            | TranslationSource::FromFnCall { .. }
            | TranslationSource::Shadowed => None,
        }
    }
}

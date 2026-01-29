//! Translation function source types.
//!
//! Defines how translation functions are obtained in code:
//! - Direct: `const t = useTranslations("Namespace")`
//! - FromProps: `function Component({ t }: Props)`
//! - FromFnCall: `const fn = (t) => { ... }`
//! - Shadowed: parameter that shadows an outer binding

/// Source of a translation function binding (Phase 2: Extraction).
///
/// Tracks how a translation function variable was obtained, which determines
/// which namespaces to search when resolving keys in Phase 3.
///
/// # Phase Context
///
/// - **Created in**: Phase 2 by `FileAnalyzer` while tracking variable bindings
/// - **Used in**: Phase 2 to determine namespaces for translation calls
/// - **Consumed in**: Phase 3 to resolve keys against the correct namespaces
///
/// # Examples
///
/// ```ignore
/// // Direct - single known namespace
/// const t = useTranslations("Common");
/// t("submit");  // Look in Common.submit
///
/// // FromProps - multiple possible namespaces
/// function Button({ t }) {  // Used with "Common" and "Errors"
///   t("cancel");  // Check both Common.cancel and Errors.cancel
/// }
///
/// // FromFnCall - multiple possible namespaces
/// const makeLabels = (t) => ({ save: t("save") });
/// makeLabels(useTranslations("Actions"));  // Actions.save
///
/// // Shadowed - don't track
/// const t = useTranslations("Outer");
/// function inner(t) {  // Shadows outer t
///   t("key");  // Don't track (not a translation function)
/// }
/// ```
#[derive(Debug, Clone)]
pub enum TranslationSource {
    /// Direct binding from a translation hook call.
    ///
    /// Example: `const t = useTranslations("Namespace")`
    ///
    /// Has a single, known namespace (or `None` if called without argument).
    Direct { namespace: Option<String> },

    /// From React component props.
    ///
    /// Example: `function Component({ t }: Props)`
    ///
    /// May have multiple possible namespaces from different JSX call sites.
    /// Collected in Phase 1 when we see `<Component t={someT} />`.
    FromProps {
        /// All possible namespaces from call sites where this component is used.
        /// Empty if no call sites found (unusual, will still generate warnings).
        namespaces: Vec<Option<String>>,
    },

    /// From function call argument (non-React).
    ///
    /// Example: `const makeLabels = (t) => { ... }; makeLabels(t)`
    ///
    /// Similar to `FromProps`, may have multiple possible namespaces from different call sites.
    /// Collected in Phase 1 when we see `someFunction(translationVar)`.
    FromFnCall {
        /// All possible namespaces from call sites where this function is called.
        /// Empty if no call sites found (will still generate warnings).
        namespaces: Vec<Option<String>>,
    },

    /// Shadowed binding (parameter shadows outer translation binding).
    ///
    /// Example:
    /// ```ignore
    /// const t = useTranslations("Outer");
    /// function inner(t) {  // This 't' is not a translation function
    ///   t("key");  // Should NOT be tracked
    /// }
    /// ```
    ///
    /// Used when an inner function parameter shadows an outer translation binding.
    /// Calls using this binding are ignored (not tracked as translation calls).
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

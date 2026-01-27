//! Translation function source types.
//!
//! Defines how translation functions are obtained in code:
//! - Direct: `const t = useTranslations("Namespace")`
//! - FromProps: `function Component({ t }: Props)`
//! - FromFnCall: `const fn = (t) => { ... }`
//! - Shadowed: parameter that shadows an outer binding

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

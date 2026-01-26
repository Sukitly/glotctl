//! Raw translation call data collected during AST traversal.
//!
//! This module defines the intermediate representation for translation calls
//! collected during AST traversal. The actual resolution to UsedKey/DynamicKeyWarning
//! happens in the resolve phase.

use crate::analysis::SourceContext;
use crate::extraction::extract::{TranslationSource, ValueSource};

/// Translation call kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationCallKind {
    /// Direct call: t("key")
    Direct,
    /// Method call: t.raw("key"), t.rich("key"), t.markup("key")
    Method(String),
}

/// Raw translation call data collected during AST traversal.
///
/// Contains all information needed for the resolve phase to generate
/// UsedKey and DynamicKeyWarning results.
#[derive(Debug, Clone)]
pub struct RawTranslationCall {
    /// Source code context (location + source_line + comment_style).
    pub context: SourceContext,

    /// Translation function source (contains all possible namespaces).
    pub translation_source: TranslationSource,

    /// Analyzed argument expression.
    pub argument: ValueSource,

    /// Call kind (direct vs method).
    pub call_kind: TranslationCallKind,
}

//! Translation key extraction module.
//!
//! This module analyzes TSX/JSX files to extract translation keys used in code.
//! It handles:
//!
//! - Static keys: `t("key.name")`
//! - Dynamic keys: `t(variable)` or `t(\`prefix.${id}\`)`
//! - Namespace extraction: `const t = useTranslations("namespace")`
//! - Schema-based translations: factory patterns for complex key generation
//! - glot-message-keys annotations: explicit key declarations for dynamic keys
//!
//! The extractor uses the `ValueAnalyzer` to resolve dynamic expressions and
//! track variable bindings across the codebase.

// Expose internal modules for FileAnalyzer
pub(crate) mod annotation_store;
pub(crate) mod binding_context;
pub(crate) mod translation_source;

mod result;

// Public API
pub use result::{DynamicKeyReason, DynamicKeyWarning, KeyExtractionResult, UsedKey};

#[cfg(test)]
mod tests;

//! File-level extraction (Phase 2).
//!
//! This module handles the second phase of extraction: analyzing individual files
//! to extract translation keys, detect hardcoded text, and track translation function bindings.
//!
//! **No comment processing happens here** - that's Phase 3 (resolve).

pub mod binding_context;
pub mod file_analyzer;
pub mod translation_source;
pub mod value_analyzer;
pub mod value_source;

pub use binding_context::BindingContext;
pub use file_analyzer::FileAnalyzer;
pub use translation_source::TranslationSource;
pub use value_analyzer::ValueAnalyzer;
pub use value_source::{ResolvedKey, ValueSource};

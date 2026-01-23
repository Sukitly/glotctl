//! File analyzer for extracting translation keys and hardcoded text.
//!
//! This module handles the second phase of extraction: analyzing individual files
//! to extract translation keys, detect hardcoded text, and track translation function bindings.

pub mod binding_context;
pub mod file_analyzer;
pub mod translation_source;

pub use binding_context::BindingContext;
pub use file_analyzer::FileAnalyzer;
pub use translation_source::TranslationSource;

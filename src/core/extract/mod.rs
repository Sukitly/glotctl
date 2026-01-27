//! File-level extraction (Phase 2).
//!
//! This module handles the second phase of extraction: analyzing individual files
//! to collect raw translation calls, detect hardcoded text, and track translation function bindings.
//!
//! The actual resolution to UsedKey/DynamicKeyWarning happens in Phase 3 (resolve).

pub mod binding_context;
pub mod file_analyzer;
pub mod raw_call;
pub mod translation_source;
pub mod value_analyzer;
pub mod value_source;

pub use binding_context::BindingContext;
pub use file_analyzer::FileAnalyzer;
pub use raw_call::{RawTranslationCall, TranslationCallKind};
pub use translation_source::TranslationSource;
pub use value_analyzer::ValueAnalyzer;
pub use value_source::ValueSource;

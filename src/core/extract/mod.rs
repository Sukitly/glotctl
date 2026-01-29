//! Phase 2: Extraction - File-level raw data collection.
//!
//! This module handles the second phase of the analysis pipeline: analyzing individual
//! files to collect raw translation calls, detect hardcoded text, and track translation
//! function bindings.
//!
//! The raw data collected here (RawTranslationCall, HardcodedTextIssue) is then resolved
//! in Phase 3 to produce final ResolvedKeyUsage and UnresolvedKeyUsage results.

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

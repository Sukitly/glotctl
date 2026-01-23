//! Dynamic value resolution for translation keys.
//!
//! This module provides tools for analyzing and resolving dynamic translation key expressions,
//! such as template literals, object access, and array iterations.

pub mod annotation_store;
pub mod value_analyzer;
pub mod value_source;

pub use annotation_store::AnnotationStore;
pub use value_analyzer::ValueAnalyzer;
pub use value_source::{ResolvedKey, ValueSource};

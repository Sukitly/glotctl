//! Translation key extraction and analysis.
//!
//! This module provides tools for extracting translation keys from TypeScript/JavaScript code:
//!
//! ## Module Structure
//!
//! - `registry`: Cross-file dependency collection (Phase 1)
//! - `analyzer`: File analysis and key extraction (Phase 2)
//! - `resolver`: Dynamic value resolution
//! - `schema`: Schema function handling
//! - `utils`: Helper functions and utilities
//!
//! ## Extraction Pipeline
//!
//! 1. **Registry Collection** (`registry::RegistryCollector`)
//!    - First pass: collect schema functions, key objects, translation props
//!    - Build cross-file dependency registries
//!
//! 2. **File Analysis** (`analyzer::FileAnalyzer`)
//!    - Second pass: analyze each file
//!    - Extract translation keys, detect hardcoded text
//!    - Resolve dynamic keys using registries
//!
//! 3. **Result Aggregation**
//!    - Combine results from all files
//!    - Ready for rule checking

pub mod analyzer;
pub mod registry;
pub mod resolver;
pub mod results;
pub mod schema;
pub mod utils;

// Re-export commonly used types from results module
pub use results::{DynamicKeyReason, DynamicKeyWarning, KeyExtractionResult, UsedKey};

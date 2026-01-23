//! Translation key extraction and analysis.
//!
//! This module provides a three-phase extraction pipeline for analyzing translation keys:
//!
//! ## Module Structure
//!
//! - `collect`: Phase 1 - Cross-file dependency collection
//! - `extract`: Phase 2 - File-level extraction
//! - `resolve`: Phase 3 - Comment application and final results
//! - `schema`: Schema function handling
//! - `utils`: Helper functions and utilities
//!
//! ## Three-Phase Extraction Pipeline
//!
//! 1. **Collection Phase** (`collect::RegistryCollector`)
//!    - First AST pass: collect schema functions, key objects, translation props
//!    - Build cross-file dependency registries
//!
//! 2. **Extraction Phase** (`extract::FileExtractor`)
//!    - Second AST pass: extract translation keys and detect hardcoded text
//!    - Output: Raw results (without comment processing)
//!
//! 3. **Resolution Phase** (`resolve::Resolver`)
//!    - No AST traversal: apply disable directives and glot-message-keys annotations
//!    - Output: Final results ready for rule checking

pub mod collect;
pub mod extract;
pub mod pipeline;
pub mod resolve;
pub mod results;
pub mod schema;
pub mod utils;

// Re-export commonly used types from results module
pub use results::{DynamicKeyReason, DynamicKeyWarning, KeyExtractionResult, UsedKey};

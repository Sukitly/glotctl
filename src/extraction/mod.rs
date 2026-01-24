//! Translation key extraction and analysis.
//!
//! This module provides a two-phase extraction pipeline for analyzing translation keys:
//!
//! ## Module Structure
//!
//! - `collect`: Phase 1 - Cross-file dependency and comment collection
//! - `extract`: Phase 2 - File-level extraction with comment application
//! - `schema`: Schema function handling
//! - `utils`: Helper functions and utilities
//!
//! ## Two-Phase Extraction Pipeline
//!
//! 1. **Collection Phase** (`collect::RegistryCollector` + `collect::CommentCollector`)
//!    - First AST pass: collect schema functions, key objects, translation props
//!    - Parse comments for disable directives and glot-message-keys annotations
//!    - Build cross-file dependency registries
//!
//! 2. **Extraction Phase** (`extract::FileAnalyzer`)
//!    - Second AST pass: extract translation keys and detect hardcoded text
//!    - Apply disable directives and glot-message-keys during extraction
//!    - Output: Final results ready for rule checking

pub mod collect;
pub mod extract;
pub mod pipeline;
pub mod results;
pub mod schema;
pub mod utils;

// Re-export commonly used types from results module
pub use results::{DynamicKeyReason, DynamicKeyWarning, KeyExtractionResult, UsedKey};

//! Translation key extraction and analysis.
//!
//! This module provides a three-phase extraction pipeline for analyzing translation keys:
//!
//! ## Module Structure
//!
//! - `collect`: Phase 1 - Cross-file dependency and comment collection
//! - `extract`: Phase 2 - File-level raw data collection
//! - `resolve`: Phase 3 - Resolution to final ResolvedKeyUsage/UnresolvedKeyUsage
//! - `schema`: Schema function handling
//! - `utils`: Helper functions and utilities
//!
//! ## Three-Phase Extraction Pipeline
//!
//! 1. **Collection Phase** (`collect::RegistryCollector` + `collect::CommentCollector`)
//!    - First AST pass: collect schema functions, key objects, translation props
//!    - Parse comments for disable directives and glot-message-keys annotations
//!    - Build cross-file dependency registries
//!
//! 2. **Extraction Phase** (`extract::FileAnalyzer`)
//!    - Second AST pass: collect raw translation calls and detect hardcoded text
//!    - Output: RawTranslationCall with ValueSource (no resolution yet)
//!
//! 3. **Resolution Phase** (`resolve::resolve_translation_calls`)
//!    - Resolve ValueSource to static keys
//!    - Expand schema calls
//!    - Apply glot-message-keys expansion
//!    - Generate warnings for unresolvable dynamic keys
//!    - Output: Final ResolvedKeyUsage/UnresolvedKeyUsage results

pub mod collect;
pub mod extract;
pub mod resolve;
pub mod results;
pub mod schema;
pub mod utils;

// Re-export commonly used types from results module
pub use results::{AllKeyUsages, UnresolvedKeyReason, UnresolvedKeyUsage};

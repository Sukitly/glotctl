//! Phase 1: Collection - Cross-file dependencies and comments.
//!
//! This module handles the first phase of the analysis pipeline:
//! - Collecting schema functions, key objects, string arrays, translation props/calls
//! - Collecting all glot comments (disable directives and glot-message-keys annotations)
//!
//! This data is collected in a single AST pass per file and is used by Phase 2 (Extraction)
//! and Phase 3 (Resolution) to resolve translation calls and detect issues.

pub mod comments;
pub mod registry;
pub mod types;

pub use comments::collector::CommentCollector;
pub use comments::directive::Directive;
pub use registry::RegistryCollector;
pub use types::*;

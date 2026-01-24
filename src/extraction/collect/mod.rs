//! Phase 1: Collection of cross-file dependencies and comments.
//!
//! This module handles the first phase of extraction:
//! - Collecting schema functions, key objects, string arrays, translation props/calls
//! - Collecting all glot comments (disable directives and message-keys annotations)

mod comments;
pub mod registry;
pub mod types;

pub use comments::collector::CommentCollector;
pub use registry::RegistryCollector;
pub use types::*;

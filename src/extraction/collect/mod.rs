//! Phase 1: Collection of cross-file dependencies and comments.
//!
//! This module handles the first phase of extraction:
//! - Collecting schema functions, key objects, string arrays, translation props/calls
//! - Collecting all glot comments (disable directives and message-keys annotations)

pub mod collector;
pub mod comment_collector;
mod comments;
pub mod types;

pub use collector::RegistryCollector;
pub use comment_collector::CommentCollector;
pub use types::*;

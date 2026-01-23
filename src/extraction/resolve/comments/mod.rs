//! Comment parsing components for glot directives.
//!
//! This module provides components for handling glot comment types:
//! - `glot-disable` / `glot-enable` - Disable rules for ranges
//! - `glot-disable-next-line` - Disable rules for next line
//! - `glot-message-keys` - Declare expected dynamic keys
//!
//! ## Usage
//!
//! Comments are now collected in Phase 1 via `CommentCollector`:
//!
//! ```ignore
//! let file_comments = CommentCollector::collect(source, swc_comments, source_map, file_path, available_keys);
//! if file_comments.disable_context.should_ignore(line, DisableRule::Hardcoded) { ... }
//! if let Some(annotation) = file_comments.annotations.get_annotation(line) { ... }
//! ```

pub mod annotation_store;
pub mod disable_context;
pub mod parser;

pub use annotation_store::AnnotationStore;
pub use disable_context::{DisableContext, DisableRule};

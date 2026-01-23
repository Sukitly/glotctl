//! Comment parsing and storage for glot directives.
//!
//! This module provides unified handling of all glot comment types:
//! - `glot-disable` / `glot-enable` - Disable rules for ranges
//! - `glot-disable-next-line` - Disable rules for next line
//! - `glot-message-keys` - Declare expected dynamic keys
//!
//! ## Usage
//!
//! Use `CommentStore` as the primary entry point for parsing all comments:
//!
//! ```ignore
//! let store = CommentStore::parse(source, swc_comments, source_map, file_path, available_keys);
//! if store.should_ignore(line, DisableRule::Hardcoded) { ... }
//! if let Some(annotation) = store.get_annotation(line) { ... }
//! ```

pub mod annotation_store;
pub mod disable_context;
pub mod parser;
pub mod store;

pub use annotation_store::AnnotationStore;
pub use disable_context::{DisableContext, DisableRule};
pub use store::CommentStore;

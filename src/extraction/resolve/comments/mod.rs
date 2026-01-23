//! Comment parsing and storage for glot directives.
//!
//! This module provides unified handling of all glot comment types:
//! - `glot-disable` / `glot-enable` - Disable rules for ranges
//! - `glot-disable-next-line` - Disable rules for next line
//! - `glot-message-keys` - Declare expected dynamic keys

pub mod annotation_store;
pub mod disable_context;
pub mod parser;

pub use annotation_store::AnnotationStore;
pub use disable_context::{DisableContext, DisableRule};

//! Core data types used across all pipeline phases.
//!
//! This module defines the fundamental data structures for representing
//! source locations, message contexts, and comment styles.
//!
//! ## Module Structure
//!
//! - `comment_style`: CommentStyle enum (JS vs JSX comments)
//! - `message`: Message-related types (LocaleMessages, MessageEntry, ValueType)
//! - `source`: Source code location types (SourceContext, SourceLocation)

pub mod comment_style;
pub mod message;
pub mod source;

pub use comment_style::CommentStyle;
pub use message::{
    AllLocaleMessages, LocaleMessages, LocaleTypeMismatch, MessageContext, MessageEntry,
    MessageLocation, ValueType,
};
pub use source::{SourceContext, SourceLocation};

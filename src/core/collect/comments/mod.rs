//! Comment collection - Disable directives and glot-message-keys annotations.
//!
//! This module collects and parses glot-specific comments:
//! - Disable directives: `// glot-disable-next-line <rule>` (suppresses specific rule checks)
//! - glot-message-keys: `// glot-message-keys "pattern"` (declares expected dynamic keys)
//!
//! These are collected during Phase 1 alongside registry collection.
//!
//! ## Module Structure
//!
//! - `collector`: Main CommentCollector implementation
//! - `declarations`: glot-message-keys pattern parsing and expansion
//! - `directive`: Disable directive parsing
//! - `suppressions`: Suppression tracking by line and rule

pub mod collector;
mod declarations;
pub mod directive;
mod suppressions;

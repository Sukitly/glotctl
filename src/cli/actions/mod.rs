//! Actions for processing i18n issues.
//!
//! Actions convert Issues into Operations and execute them.
//! This module provides a type-safe way to handle different issue types.
//!
//! ## Architecture
//!
//! ```text
//! Issue (problem detected)
//!     ↓
//! Action (Issue → Operation conversion)
//!     ↓
//! Operation (low-level file operation)
//!     ↓
//! execute (file modification)
//! ```
//!
//! ## Actions
//!
//! - [`InsertDisableComment`]: Insert `glot-disable-next-line` comments (baseline)
//! - [`InsertMessageKeys`]: Insert `glot-message-keys` comments (fix)
//! - [`DeleteKey`]: Delete keys from JSON files (clean)
//!
//! ## Example
//!
//! ```ignore
//! use glot::actions::{Action, InsertDisableComment};
//! use glot::issues::HardcodedIssue;
//!
//! let issues: Vec<HardcodedIssue> = checkers::hardcoded(&data);
//! let stats = InsertDisableComment::run(&issues, apply)?;
//! ```

mod delete_key;
mod insert_disable_comment;
mod insert_message_keys;
mod json_editor;
mod operation;
mod traits;

pub use delete_key::DeleteKey;
pub use insert_disable_comment::InsertDisableComment;
pub use insert_message_keys::InsertMessageKeys;
pub(crate) use traits::execute_operations;
pub use traits::{Action, ActionStats};

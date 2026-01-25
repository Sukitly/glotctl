//! Rule implementations for glot.
//!
//! This module contains pure functions that check for various i18n issues.
//! Each function takes only the specific inputs it needs (not a full Context)
//! and returns a specific issue type.
//!
//! ## Module Structure
//!
//! - `helpers`: Shared types and utility functions (KeyUsageMap, KeyDisableMap, etc.)
//! - `hardcoded`: Hardcoded text detection
//! - `missing_key`: Missing translation key detection
//! - `unresolved_key`: Unresolved (dynamic) key detection
//! - `replica_lag`: Keys missing in non-primary locales
//! - `unused_key`: Defined but unused keys
//! - `orphan_key`: Keys in non-primary locales but not in primary
//! - `untranslated`: Identical values across locales
//! - `type_mismatch`: Type mismatches between locales

pub mod hardcoded;
pub mod helpers;
pub mod missing_key;
pub mod orphan_key;
pub mod replica_lag;
pub mod type_mismatch;
pub mod unresolved_key;
pub mod untranslated;
pub mod unused_key;

// Re-export all check functions for convenient access
pub use hardcoded::check_hardcoded;
pub use helpers::{build_key_disable_map, build_key_usage_map};
pub use missing_key::check_missing_key;
pub use orphan_key::check_orphan_key;
pub use replica_lag::check_replica_lag;
pub use type_mismatch::check_type_mismatch;
pub use unresolved_key::check_unresolved_key;
pub use untranslated::check_untranslated;
pub use unused_key::check_unused_key;

// ============================================================
// DEPRECATED: Legacy Checker trait for backward compatibility
// TODO: Remove after migrating baseline.rs, fix.rs, clean.rs
// ============================================================

use crate::{commands::context::CheckContext, issue::Issue};
use anyhow::Result;

/// Trait for implementing a check rule.
///
/// DEPRECATED: Use the pure `check_*` functions directly instead.
/// This trait is kept temporarily for backward compatibility with
/// baseline.rs, fix.rs, and clean.rs.
#[deprecated(note = "Use check_* functions directly instead")]
pub trait Checker {
    fn name(&self) -> &str;
    fn needs_registries(&self) -> bool {
        false
    }
    fn needs_messages(&self) -> bool {
        false
    }
    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>>;
}

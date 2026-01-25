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
pub mod missing;
pub mod orphan;
pub mod replica_lag;
pub mod type_mismatch;
pub mod unresolved;
pub mod untranslated;
pub mod unused;

// Re-export all check functions for convenient access
pub use helpers::{build_key_disable_map, build_key_usage_map};

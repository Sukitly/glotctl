//! Key extraction result types.
//!
//! This module re-exports the key usage types from `types::key_usage`.
//!
//! The main types are:
//! - `AllKeyUsages`: map of file path to key usages
//! - `UnresolvedKeyUsage`: a key that could not be statically resolved

// Re-export the key usage types
pub use crate::types::key_usage::{AllKeyUsages, UnresolvedKeyReason, UnresolvedKeyUsage};

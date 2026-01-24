//! Key extraction result types.
//!
//! This module re-exports the key usage types from `types::key_usage`.
//!
//! The main types are:
//! - `FileKeyUsages`: resolved and unresolved key usages for a single file
//! - `ResolvedKeyUsage`: a successfully resolved translation key
//! - `UnresolvedKeyUsage`: a key that could not be statically resolved

// Re-export the key usage types
pub use crate::types::key_usage::{
    AllKeyUsages, FileKeyUsages, ResolvedKeyUsage, UnresolvedKeyReason, UnresolvedKeyUsage,
};

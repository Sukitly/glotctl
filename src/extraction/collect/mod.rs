//! Registry collection for cross-file dependencies.
//!
//! This module handles the first phase of extraction: collecting schema functions,
//! key objects, string arrays, translation props, and function calls across all files.

pub mod collector;
pub mod types;

pub use collector::RegistryCollector;
pub use types::*;

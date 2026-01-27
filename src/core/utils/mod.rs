//! Utility functions and helpers for core analysis.
//!
//! This module provides shared utility functions used throughout the core analysis pipeline.
//!
//! ## Module Structure
//!
//! - `glob_matcher`: Glob pattern matching utilities
//! - `helpers`: Helper functions for namespace extraction and translation hook detection

pub mod glob_matcher;
pub mod helpers;

pub use glob_matcher::*;
pub use helpers::*;

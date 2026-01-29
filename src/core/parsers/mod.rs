//! File parsers for source code and message files.
//!
//! This module provides parsers for different file types:
//! - `json`: JSON message file parser (scans locale directories)
//! - `jsx`: JSX/TSX source file parser (uses swc for AST generation)

pub mod json;
pub mod jsx;

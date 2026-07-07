//! File parsers for source code and message files.
//!
//! This module provides parsers for different file types:
//! - `json`: JSON message file parser (scans locale directories)
//! - `jsx`: JSX/TSX source file parser (uses swc for AST generation)
//! - `astro`: Astro source parser that converts supported Astro syntax into TSX

pub mod astro;
pub mod json;
pub mod jsx;

//! Resolution phase (Phase 3) - Apply comments and generate final results.
//!
//! This module handles the third phase: applying disable directives and glot-message-keys
//! annotations to raw extraction results.
//!
//! No AST traversal happens here - all processing is post-extraction.

pub mod comments;

//! Comment collector for Phase 1.
//!
//! This module collects all glot comments (suppression directives and key declarations)
//! from a file's SingleThreadedComments during Phase 1. The collected FileComments are then
//! passed to FileAnalyzer in Phase 2 for immediate use, avoiding re-parsing.

use std::collections::HashSet;

use swc_common::{SourceMap, comments::SingleThreadedComments};

use super::types::{Declarations, FileComments, Suppressions};

/// Collects all glot comments from a file.
pub struct CommentCollector;

impl CommentCollector {
    /// Collect all glot comments from a file.
    ///
    /// This performs:
    /// 1. SWC comment parsing for suppression directives
    /// 2. Regex parsing of source text for key declarations
    ///
    /// # Arguments
    /// * `source` - Source code text (for declaration regex parsing)
    /// * `swc_comments` - SWC parsed comments (for suppression directive parsing)
    /// * `source_map` - Source map for line number lookup
    /// * `file_path` - File path (unused currently, kept for future use)
    /// * `available_keys` - Available translation keys for glob expansion
    pub fn collect(
        source: &str,
        swc_comments: &SingleThreadedComments,
        source_map: &SourceMap,
        file_path: &str,
        available_keys: &HashSet<String>,
    ) -> FileComments {
        let suppressions = Suppressions::from_comments(swc_comments, source_map);
        let declarations = Declarations::parse(file_path, source, available_keys);

        FileComments {
            suppressions,
            declarations,
        }
    }
}

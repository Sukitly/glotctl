//! Comment collector for Phase 1.
//!
//! This module collects all glot comments (disable directives and message-keys annotations)
//! from a file's SingleThreadedComments during Phase 1. The collected FileComments are then
//! passed to FileAnalyzer in Phase 2 for immediate use, avoiding re-parsing.

use std::collections::HashSet;

use swc_common::{SourceMap, comments::SingleThreadedComments};

use super::types::FileComments;
use crate::extraction::resolve::comments::{AnnotationStore, DisableContext};

/// Collects all glot comments from a file.
pub struct CommentCollector;

impl CommentCollector {
    /// Collect all glot comments from a file.
    ///
    /// This performs:
    /// 1. SWC comment parsing for disable directives
    /// 2. Regex parsing of source text for message-keys annotations
    ///
    /// # Arguments
    /// * `source` - Source code text (for annotation regex parsing)
    /// * `swc_comments` - SWC parsed comments (for disable directive parsing)
    /// * `source_map` - Source map for line number lookup
    /// * `file_path` - File path for warnings
    /// * `available_keys` - Available translation keys for glob expansion
    pub fn collect(
        source: &str,
        swc_comments: &SingleThreadedComments,
        source_map: &SourceMap,
        file_path: &str,
        available_keys: &HashSet<String>,
    ) -> FileComments {
        let disable_context = DisableContext::from_comments(swc_comments, source_map);
        let annotation_store = AnnotationStore::parse(source, file_path, available_keys);
        let pattern_warnings = annotation_store.warnings.clone();

        FileComments {
            disable_context,
            annotations: annotation_store,
            pattern_warnings,
        }
    }
}

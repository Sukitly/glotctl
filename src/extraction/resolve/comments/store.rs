//! Unified comment store for all glot directives.
//!
//! This module provides a single entry point for parsing and accessing
//! all glot comment types from a source file.

use std::collections::HashSet;

use swc_common::{comments::SingleThreadedComments, SourceMap};

use super::parser::PatternWarning;
use super::{AnnotationStore, DisableContext, DisableRule};

/// Unified storage for all glot comments in a file.
///
/// This provides a single entry point for parsing both disable directives
/// and message-keys annotations, avoiding duplicate comment traversal.
pub struct CommentStore {
    /// Disable/enable directives for rules
    disable_context: DisableContext,
    /// Message-keys annotations for dynamic expressions
    annotation_store: AnnotationStore,
}

impl CommentStore {
    /// Parse all glot comments from a file.
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
    pub fn parse(
        source: &str,
        swc_comments: &SingleThreadedComments,
        source_map: &SourceMap,
        file_path: &str,
        available_keys: &HashSet<String>,
    ) -> Self {
        let disable_context = DisableContext::from_comments(swc_comments, source_map);
        let annotation_store = AnnotationStore::parse(source, file_path, available_keys);

        Self {
            disable_context,
            annotation_store,
        }
    }

    /// Check if a line should be ignored for a specific rule.
    pub fn should_ignore(&self, line: usize, rule: DisableRule) -> bool {
        self.disable_context.should_ignore(line, rule)
    }

    /// Get message-keys annotation for a line.
    ///
    /// Checks both the current line and previous line
    /// (annotation on the line before the `t()` call).
    pub fn get_annotation(&self, line: usize) -> Option<&super::annotation_store::GlotAnnotation> {
        self.annotation_store.get_annotation(line)
    }

    /// Get parsing warnings from annotations.
    pub fn pattern_warnings(&self) -> &[PatternWarning] {
        &self.annotation_store.warnings
    }

    /// Get a reference to the annotation store (for advanced use).
    pub fn annotation_store(&self) -> &AnnotationStore {
        &self.annotation_store
    }

    /// Get a reference to the disable context (for advanced use).
    pub fn disable_context(&self) -> &DisableContext {
        &self.disable_context
    }
}

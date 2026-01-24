//! Shared utilities for command runners.
//!
//! This module contains functions that are shared between CheckRunner and CleanRunner
//! to avoid code duplication. Functions now delegate to the extraction pipeline.

use std::collections::HashSet;

use crate::{
    commands::context::{
        AllFileImports, AllHardcodedIssues, AllKeyUsages, CheckContext, Registries,
    },
    extraction::pipeline,
};

/// Build registries from cached parsed files using the extraction pipeline.
///
/// Returns (Registries, AllFileImports).
/// Requires `ctx.ensure_parsed_files()` to be called first.
pub fn build_registries(ctx: &CheckContext) -> (Registries, AllFileImports) {
    let parsed_files = ctx
        .parsed_files()
        .expect("parsed_files must be loaded before build_registries");

    let (registries, file_imports, _, _) = pipeline::run_pipeline(ctx, parsed_files);
    (registries, file_imports)
}

/// Build file analysis (key_usages + hardcoded issues) from cached parsed files using the extraction pipeline.
///
/// Performs a single AST traversal per file to generate both results.
/// Requires parsed_files, registries, file_imports, and messages to be loaded in ctx.
pub fn build_file_analysis(ctx: &CheckContext) -> (AllKeyUsages, AllHardcodedIssues) {
    let parsed_files = ctx
        .parsed_files()
        .expect("parsed_files must be loaded before build_file_analysis");

    let (_, _, key_usages, hardcoded_issues) = pipeline::run_pipeline(ctx, parsed_files);
    (key_usages, hardcoded_issues)
}

/// Collect all used translation keys from cached extractions.
///
/// Requires extractions to be loaded in ctx.
pub fn collect_used_keys(ctx: &CheckContext) -> HashSet<String> {
    let mut used_keys = HashSet::new();

    let extractions = ctx
        .extractions()
        .expect("extractions must be loaded before collect_used_keys");

    // Collect keys from resolved key usages
    for file_usages in extractions.values() {
        for resolved in &file_usages.resolved {
            used_keys.insert(resolved.key.as_str().to_string());
        }
    }

    used_keys
}

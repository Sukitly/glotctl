//! Shared utilities for command runners.
//!
//! This module contains functions that are shared between CheckRunner and CleanRunner
//! to avoid code duplication. Functions now delegate to the extraction pipeline.

use std::collections::HashSet;

use crate::{
    commands::context::{
        AllExtractions, AllFileImports, AllHardcodedIssues, CheckContext, Registries,
    },
    extraction::{extract::ValueSource, pipeline, schema::expand_schema_keys},
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

/// Build file analysis (extractions + hardcoded issues) from cached parsed files using the extraction pipeline.
///
/// Performs a single AST traversal per file to generate both results.
/// Requires parsed_files, registries, file_imports, and messages to be loaded in ctx.
pub fn build_file_analysis(ctx: &CheckContext) -> (AllExtractions, AllHardcodedIssues) {
    let parsed_files = ctx
        .parsed_files()
        .expect("parsed_files must be loaded before build_file_analysis");

    let (_, _, extractions, hardcoded_issues) = pipeline::run_pipeline(ctx, parsed_files);
    (extractions, hardcoded_issues)
}

/// Collect all used translation keys from cached extractions.
///
/// Requires registries, extractions, and messages to be loaded in ctx.
pub fn collect_used_keys(ctx: &CheckContext) -> HashSet<String> {
    let mut used_keys = HashSet::new();

    let extractions = ctx
        .extractions()
        .expect("extractions must be loaded before collect_used_keys");
    let registries = ctx
        .registries()
        .expect("registries must be loaded before collect_used_keys");
    let messages = ctx
        .messages()
        .expect("messages must be loaded before collect_used_keys");
    let primary_messages = messages
        .primary_messages
        .as_ref()
        .expect("primary_messages must exist before collect_used_keys");

    for extraction in extractions.values() {
        // Static keys
        for key in &extraction.used_keys {
            used_keys.insert(key.full_key.clone());
        }

        // Schema keys
        for call in &extraction.schema_calls {
            let mut visited = HashSet::new();
            let expand_result = expand_schema_keys(
                &call.schema_name,
                &call.namespace,
                &registries.schema,
                &mut visited,
            );
            for key in expand_result.keys {
                used_keys.insert(key.full_key.clone());
            }
        }

        // Resolved keys
        for resolved_key in &extraction.resolved_keys {
            if matches!(resolved_key.source, ValueSource::Literal(_)) {
                continue;
            }

            if let Ok(keys) = resolved_key.source.resolve_keys() {
                for key in keys {
                    let full_key = match &resolved_key.namespace {
                        Some(ns) => format!("{}.{}", ns, key),
                        None => key,
                    };
                    if primary_messages.contains_key(&full_key) {
                        used_keys.insert(full_key);
                    }
                }
            }
        }
    }

    used_keys
}

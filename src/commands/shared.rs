//! Shared utilities for command runners.
//!
//! This module contains functions that are shared between CheckRunner and CleanRunner
//! to avoid code duplication.

use std::collections::{HashMap, HashSet};

use swc_ecma_visit::VisitWith;

use crate::{
    checkers::{
        key_objects::{
            KeyObjectCollector, make_registry_key, make_translation_fn_call_key,
            make_translation_prop_key,
        },
        schema::{SchemaFunctionCollector, expand_schema_keys},
        value_source::ValueSource,
    },
    commands::{
        check::extract_translation_keys,
        context::{AllExtractions, AllFileImports, CheckContext, Registries},
    },
    issue::Issue,
    parsers::jsx::parse_jsx_file,
};

use std::path::Path;

/// Build registries by parsing all source files.
///
/// Returns (Registries, AllFileImports, parse_errors).
/// Parse errors are logged if verbose is true.
pub fn build_registries(ctx: &CheckContext) -> (Registries, AllFileImports, Vec<Issue>) {
    let mut schema = HashMap::new();
    let mut key_object = HashMap::new();
    let mut key_array = HashMap::new();
    let mut string_array = HashMap::new();
    let mut translation_prop = HashMap::new();
    let mut translation_fn_call = HashMap::new();
    let mut default_exports = HashMap::new();
    let mut file_imports: AllFileImports = HashMap::new();
    let mut errors = Vec::new();

    for file_path in &ctx.files {
        let parsed = match parse_jsx_file(Path::new(file_path)) {
            Ok(p) => p,
            Err(e) => {
                if ctx.verbose {
                    eprintln!("Warning: {} - {}", file_path, e);
                }
                errors.push(Issue::parse_error(file_path, &e.to_string()));
                continue;
            }
        };

        // Schema registry
        let mut schema_collector = SchemaFunctionCollector::new(file_path);
        parsed.module.visit_with(&mut schema_collector);
        for func in schema_collector.functions {
            if !schema.contains_key(&func.name) {
                schema.insert(func.name.clone(), func);
            }
        }

        // Key registry
        let mut key_collector = KeyObjectCollector::new(file_path);
        parsed.module.visit_with(&mut key_collector);
        file_imports.insert(file_path.clone(), key_collector.imports);

        for obj in key_collector.objects {
            let key = make_registry_key(&obj.file_path, &obj.name);
            key_object.insert(key, obj);
        }

        for arr in key_collector.arrays {
            let key = make_registry_key(&arr.file_path, &arr.name);
            key_array.insert(key, arr);
        }

        for str_arr in key_collector.string_arrays {
            let key = make_registry_key(&str_arr.file_path, &str_arr.name);
            string_array.insert(key, str_arr);
        }

        // Translation prop registry: merge namespaces for same component.prop
        for prop in key_collector.translation_props {
            let key = make_translation_prop_key(&prop.component_name, &prop.prop_name);
            translation_prop
                .entry(key)
                .and_modify(
                    |existing: &mut crate::checkers::key_objects::TranslationProp| {
                        // Merge namespaces from different call sites
                        for ns in &prop.namespaces {
                            if !existing.namespaces.contains(ns) {
                                existing.namespaces.push(ns.clone());
                            }
                        }
                    },
                )
                .or_insert(prop);
        }

        // Translation function call registry: merge namespaces for same fn.arg_index
        for fn_call in key_collector.translation_fn_calls {
            let key = make_translation_fn_call_key(
                &fn_call.fn_file_path,
                &fn_call.fn_name,
                fn_call.arg_index,
            );
            translation_fn_call
                .entry(key)
                .and_modify(
                    |existing: &mut crate::checkers::key_objects::TranslationFnCall| {
                        // Merge namespaces from different call sites
                        for ns in &fn_call.namespaces {
                            if !existing.namespaces.contains(ns) {
                                existing.namespaces.push(ns.clone());
                            }
                        }
                    },
                )
                .or_insert(fn_call);
        }

        // Default export registry: track which function is the default export
        if let Some(name) = key_collector.default_export_name {
            default_exports.insert(file_path.clone(), name);
        }
    }

    let registries = Registries {
        schema,
        key_object,
        key_array,
        string_array,
        translation_prop,
        translation_fn_call,
        default_exports,
    };

    (registries, file_imports, errors)
}

/// Build extractions by parsing all source files once.
///
/// Returns (extractions, parse_errors).
/// Requires registries, file_imports, and messages to be loaded in ctx.
pub fn build_extractions(ctx: &CheckContext) -> (AllExtractions, Vec<Issue>) {
    let mut extractions = HashMap::new();
    let mut errors = Vec::new();

    let registries = ctx
        .registries()
        .expect("registries must be loaded before build_extractions");
    let file_imports = ctx
        .file_imports()
        .expect("file_imports must be loaded before build_extractions");
    let messages = ctx
        .messages()
        .expect("messages must be loaded before build_extractions");

    let available_keys: HashSet<String> = messages
        .primary_messages
        .as_ref()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    for file_path in &ctx.files {
        let imports = file_imports.get(file_path).cloned().unwrap_or_default();

        match extract_translation_keys(file_path, registries, &imports, &available_keys) {
            Ok(result) => {
                extractions.insert(file_path.clone(), result);
            }
            Err(e) => {
                errors.push(Issue::parse_error(file_path, &e.to_string()));
            }
        }
    }

    (extractions, errors)
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

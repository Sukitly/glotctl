//! Shared utilities for command runners.
//!
//! This module contains functions that are shared between CheckRunner and CleanRunner
//! to avoid code duplication.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use swc_ecma_visit::VisitWith;

use crate::{
    checkers::{
        key_objects::{
            FileImports, TranslationProp, make_registry_key, make_translation_fn_call_key,
            make_translation_prop_key, resolve_import_path,
        },
        registry_collector::RegistryCollector,
        schema::expand_schema_keys,
        value_source::ValueSource,
    },
    commands::{
        check::extract_translation_keys,
        context::{AllExtractions, AllFileImports, CheckContext, Registries},
    },
};

/// Build registries from cached parsed files.
///
/// Returns (Registries, AllFileImports).
/// Requires `ctx.ensure_parsed_files()` to be called first.
pub fn build_registries(ctx: &CheckContext) -> (Registries, AllFileImports) {
    let parsed_files = ctx
        .parsed_files()
        .expect("parsed_files must be loaded before build_registries");

    let mut schema = HashMap::new();
    let mut key_object = HashMap::new();
    let mut key_array = HashMap::new();
    let mut string_array = HashMap::new();
    let mut translation_prop = HashMap::new();
    let mut translation_fn_call = HashMap::new();
    let mut default_exports = HashMap::new();
    let mut file_imports: AllFileImports = HashMap::new();
    let mut translation_props_by_file: Vec<(String, Vec<TranslationProp>)> = Vec::new();

    for file_path in &ctx.files {
        let Some(parsed) = parsed_files.get(file_path) else {
            // File failed to parse - already recorded as error in ensure_parsed_files
            continue;
        };

        // Single traversal: collect both schema functions and key objects
        let mut collector = RegistryCollector::new(file_path);
        parsed.module.visit_with(&mut collector);

        // Process schema functions
        for func in collector.schema_functions {
            if !schema.contains_key(&func.name) {
                schema.insert(func.name.clone(), func);
            }
        }

        // Process imports
        file_imports.insert(file_path.clone(), collector.imports);

        // Process key objects
        for obj in collector.objects {
            let key = make_registry_key(&obj.file_path, &obj.name);
            key_object.insert(key, obj);
        }

        for arr in collector.arrays {
            let key = make_registry_key(&arr.file_path, &arr.name);
            key_array.insert(key, arr);
        }

        for str_arr in collector.string_arrays {
            let key = make_registry_key(&str_arr.file_path, &str_arr.name);
            string_array.insert(key, str_arr);
        }

        translation_props_by_file.push((file_path.clone(), collector.translation_props));

        // Translation function call registry: merge namespaces for same fn.arg_index
        for fn_call in collector.translation_fn_calls {
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
        if let Some(name) = collector.default_export_name {
            default_exports.insert(file_path.clone(), name);
        }
    }

    // Translation prop registry: merge namespaces for same component.prop
    // Map default-imported component names to their default export names where possible.
    for (file_path, props) in translation_props_by_file {
        let imports = file_imports.get(&file_path).cloned().unwrap_or_default();
        for mut prop in props {
            let resolved_component_name = resolve_component_name_for_prop(
                &file_path,
                &prop.component_name,
                &imports,
                &default_exports,
            );
            prop.component_name = resolved_component_name;
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

    (registries, file_imports)
}

fn resolve_component_name_for_prop(
    file_path: &str,
    component_name: &str,
    imports: &FileImports,
    default_exports: &HashMap<String, String>,
) -> String {
    let Some(import) = imports
        .iter()
        .find(|i| i.local_name == component_name && i.imported_name == "default")
    else {
        return component_name.to_string();
    };

    let Some(target_path) = resolve_import_path(Path::new(file_path), &import.module_path) else {
        return component_name.to_string();
    };

    default_exports
        .get(&target_path)
        .cloned()
        .unwrap_or_else(|| component_name.to_string())
}

/// Build extractions from cached parsed files.
///
/// Returns extractions map.
/// Requires parsed_files, registries, file_imports, and messages to be loaded in ctx.
pub fn build_extractions(ctx: &CheckContext) -> AllExtractions {
    let parsed_files = ctx
        .parsed_files()
        .expect("parsed_files must be loaded before build_extractions");
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

    let mut extractions = HashMap::new();

    for file_path in &ctx.files {
        let Some(parsed) = parsed_files.get(file_path) else {
            // File failed to parse - already recorded as error in ensure_parsed_files
            continue;
        };

        let imports = file_imports.get(file_path).cloned().unwrap_or_default();
        let result =
            extract_translation_keys(file_path, parsed, registries, &imports, &available_keys);
        extractions.insert(file_path.clone(), result);
    }

    extractions
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

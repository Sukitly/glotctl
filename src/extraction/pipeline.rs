//! Two-phase extraction pipeline (Biome-style).
//!
//! This module coordinates the two phases of translation key extraction:
//! 1. **Collection**: Build cross-file registries AND collect all glot comments
//! 2. **Extraction**: Extract keys and detect hardcoded text using collected comments

use std::collections::HashMap;

use crate::{commands::context::CheckContext, parsers::jsx::ParsedJSX};

use super::{
    collect::{CommentCollector, FileImports, RegistryCollector},
    extract::FileAnalyzer,
};

use swc_ecma_visit::VisitWith;

/// Run the complete extraction pipeline.
///
/// Returns (Registries, AllFileImports, AllExtractions, AllHardcodedIssues)
pub fn run_pipeline(
    ctx: &CheckContext,
    parsed_files: &HashMap<String, ParsedJSX>,
) -> (
    crate::commands::context::Registries,
    crate::commands::context::AllFileImports,
    crate::commands::context::AllExtractions,
    crate::commands::context::AllHardcodedIssues,
) {
    let available_keys = ctx
        .messages()
        .and_then(|m| m.primary_messages.as_ref())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    // Phase 1: Collect registries AND comments (Biome-style: comments collected first)
    let (registries, file_imports, file_comments) =
        collect_registries_and_comments(parsed_files, &available_keys);

    // Phase 2: Extract from files using collected comments
    let (extractions, hardcoded_issues) = extract_from_files(
        &ctx.files,
        parsed_files,
        &registries,
        &file_imports,
        &file_comments,
        &ctx.config.checked_attributes,
        &ctx.ignore_texts,
        &available_keys,
    );

    (registries, file_imports, extractions, hardcoded_issues)
}

/// Phase 1: Collect registries AND comments from all files (Biome-style).
fn collect_registries_and_comments(
    parsed_files: &HashMap<String, ParsedJSX>,
    available_keys: &std::collections::HashSet<String>,
) -> (
    crate::commands::context::Registries,
    crate::commands::context::AllFileImports,
    super::collect::types::AllFileComments,
) {
    use super::collect::types::*;

    let mut schema = HashMap::new();
    let mut key_object = HashMap::new();
    let mut key_array = HashMap::new();
    let mut string_array = HashMap::new();
    let mut translation_prop = HashMap::new();
    let mut translation_fn_call = HashMap::new();
    let mut default_exports = HashMap::new();
    let mut file_imports: crate::commands::context::AllFileImports = HashMap::new();
    let mut file_comments: AllFileComments = HashMap::new();
    let mut translation_props_by_file: Vec<(String, Vec<TranslationProp>)> = Vec::new();

    for (file_path, parsed) in parsed_files {
        // Collect registries
        let mut collector = RegistryCollector::new(file_path);
        parsed.module.visit_with(&mut collector);

        // Collect comments (Biome-style: in same phase as registries)
        let comments = CommentCollector::collect(
            &parsed.source,
            &parsed.comments,
            &parsed.source_map,
            file_path,
            available_keys,
        );
        file_comments.insert(file_path.clone(), comments);

        // Schema functions
        for func in collector.schema_functions {
            if !schema.contains_key(&func.name) {
                schema.insert(func.name.clone(), func);
            }
        }

        // Imports
        file_imports.insert(file_path.clone(), collector.imports);

        // Key objects
        for obj in collector.objects {
            let key = make_registry_key(&obj.file_path, &obj.name);
            key_object.insert(key, obj);
        }

        // Key arrays
        for arr in collector.arrays {
            let key = make_registry_key(&arr.file_path, &arr.name);
            key_array.insert(key, arr);
        }

        // String arrays
        for str_arr in collector.string_arrays {
            let key = make_registry_key(&str_arr.file_path, &str_arr.name);
            string_array.insert(key, str_arr);
        }

        translation_props_by_file.push((file_path.clone(), collector.translation_props));

        // Translation function calls
        for fn_call in collector.translation_fn_calls {
            let key = make_translation_fn_call_key(
                &fn_call.fn_file_path,
                &fn_call.fn_name,
                fn_call.arg_index,
            );
            translation_fn_call
                .entry(key)
                .and_modify(|existing: &mut TranslationFnCall| {
                    for ns in &fn_call.namespaces {
                        if !existing.namespaces.contains(ns) {
                            existing.namespaces.push(ns.clone());
                        }
                    }
                })
                .or_insert(fn_call);
        }

        // Default exports
        if let Some(name) = collector.default_export_name {
            default_exports.insert(file_path.clone(), name);
        }
    }

    // Resolve translation props
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
                .and_modify(|existing: &mut TranslationProp| {
                    for ns in &prop.namespaces {
                        if !existing.namespaces.contains(ns) {
                            existing.namespaces.push(ns.clone());
                        }
                    }
                })
                .or_insert(prop);
        }
    }

    let registries = crate::commands::context::Registries {
        schema,
        key_object,
        key_array,
        string_array,
        translation_prop,
        translation_fn_call,
        default_exports,
    };

    (registries, file_imports, file_comments)
}

fn resolve_component_name_for_prop(
    file_path: &str,
    component_name: &str,
    imports: &FileImports,
    default_exports: &HashMap<String, String>,
) -> String {
    use super::collect::types::resolve_import_path;
    use std::path::Path;

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

/// Phase 2: Extract translation keys and hardcoded issues from all files using collected comments.
fn extract_from_files(
    files: &std::collections::HashSet<String>,
    parsed_files: &HashMap<String, ParsedJSX>,
    registries: &crate::commands::context::Registries,
    file_imports: &crate::commands::context::AllFileImports,
    file_comments: &super::collect::types::AllFileComments,
    checked_attributes: &[String],
    ignore_texts: &std::collections::HashSet<String>,
    available_keys: &std::collections::HashSet<String>,
) -> (
    crate::commands::context::AllExtractions,
    crate::commands::context::AllHardcodedIssues,
) {
    let mut extractions = HashMap::new();
    let mut hardcoded_issues = HashMap::new();

    for file_path in files {
        let Some(parsed) = parsed_files.get(file_path) else {
            continue;
        };

        let imports = file_imports.get(file_path).cloned().unwrap_or_default();
        let comments = file_comments
            .get(file_path)
            .expect("comments should be collected in Phase 1");

        let analyzer = FileAnalyzer::new(
            file_path,
            &parsed.source_map,
            comments,
            checked_attributes,
            ignore_texts,
            registries,
            &imports,
            available_keys,
        );
        let result = analyzer.analyze(&parsed.module);

        extractions.insert(file_path.clone(), result.extraction);
        hardcoded_issues.insert(file_path.clone(), result.hardcoded_issues);
    }

    (extractions, hardcoded_issues)
}

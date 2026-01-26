use std::{
    cell::OnceCell,
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow};
use swc_ecma_visit::VisitWith;

use crate::{
    args::CommonArgs,
    config::{Config, load_config},
    extraction::{
        collect::{
            AllFileComments, CommentCollector, FileImports, RegistryCollector, TranslationFnCall,
            TranslationProp, make_registry_key, make_translation_fn_call_key,
            make_translation_prop_key, resolve_import_path,
        },
        extract::FileAnalyzer,
        resolve::resolve_translation_calls,
    },
    file_scanner::scan_files,
    parsers::{
        json::{MessageMap, scan_message_files},
        jsx::{ParsedJSX, parse_jsx_source},
    },
    types::issue::{HardcodedIssue, ParseErrorIssue},
};

use std::collections::HashMap;

// Re-export types from extraction module for convenience
pub use crate::extraction::AllKeyUsages;
pub use crate::extraction::collect::{AllFileImports, Registries};

/// Type alias for all hardcoded issues (one vec per file).
pub type AllHardcodedIssues = HashMap<String, Vec<HardcodedIssue>>;

/// Aggregated message data from all locale files.
pub struct MessageData {
    pub all_messages: HashMap<String, MessageMap>,
    pub primary_messages: MessageMap,
}

pub struct SourceMetadata {
    pub registries: Registries,
    pub file_imports: AllFileImports,
    pub file_comments: AllFileComments,
}

pub struct ResolvedData {
    pub key_usages: AllKeyUsages,
    pub hardcoded_issues: AllHardcodedIssues,
}

pub struct CheckContext {
    // Basic data (set at initialization)
    pub config: Config,
    pub root_dir: PathBuf,
    pub files: HashSet<String>,
    pub ignore_texts: HashSet<String>,
    pub verbose: bool,

    parsed_files: OnceCell<HashMap<String, ParsedJSX>>,
    parsed_files_errors: OnceCell<Vec<ParseErrorIssue>>,
    source_metadata: OnceCell<SourceMetadata>,
    resolved_data: OnceCell<ResolvedData>,
    messages: OnceCell<MessageData>,
    used_keys: OnceCell<HashSet<String>>,
}

impl CheckContext {
    /// Create a new CheckContext with basic data from command line args.
    pub fn new(args: &CommonArgs) -> Result<Self> {
        let root_dir = args.path.clone();
        let path = args
            .path
            .to_str()
            .with_context(|| anyhow!("Invalid path: {:?}", args.path))?;

        let config_result = load_config(Path::new(path))?;

        // In verbose mode, inform user if using default config
        if args.verbose && !config_result.from_file {
            eprintln!("Note: No .glotrc.json found, using default configuration");
        }

        let config = config_result.config;
        let scan_result = scan_files(
            path,
            &config.includes,
            &config.ignores,
            config.ignore_test_files,
            args.verbose,
        );

        if scan_result.skipped_count > 0 {
            eprintln!(
                "Warning: {} path(s) skipped due to access errors{}",
                scan_result.skipped_count,
                if args.verbose {
                    ""
                } else {
                    " (use -v for details)"
                }
            );
        }

        let ignore_texts = config.ignore_texts.iter().cloned().collect();

        Ok(Self {
            config,
            root_dir,
            files: scan_result.files,
            ignore_texts,
            verbose: args.verbose,
            parsed_files: OnceCell::new(),
            parsed_files_errors: OnceCell::new(),
            source_metadata: OnceCell::new(),
            resolved_data: OnceCell::new(),
            messages: OnceCell::new(),
            used_keys: OnceCell::new(),
        })
    }

    pub fn parsed_files(&self) -> &HashMap<String, ParsedJSX> {
        self.parsed_files.get_or_init(|| {
            let mut parsed = HashMap::new();
            let mut errors = Vec::new();

            for file_path in &self.files {
                match std::fs::read_to_string(file_path) {
                    Ok(code) => match parse_jsx_source(code, file_path) {
                        Ok(p) => {
                            parsed.insert(file_path.clone(), p);
                        }
                        Err(e) => {
                            if self.verbose {
                                eprintln!("Warning: {} - {}", file_path, e);
                            }
                            errors.push(ParseErrorIssue {
                                file_path: file_path.clone(),
                                error: e.to_string(),
                            });
                        }
                    },
                    Err(e) => {
                        if self.verbose {
                            eprintln!("Warning: {} - {}", file_path, e);
                        }
                        errors.push(ParseErrorIssue {
                            file_path: file_path.clone(),
                            error: format!("Failed to read file: {}", e),
                        });
                    }
                }
            }

            let _ = self.parsed_files_errors.set(errors);
            parsed
        })
    }

    pub fn parsed_files_errors(&self) -> &Vec<ParseErrorIssue> {
        self.parsed_files_errors.get_or_init(Vec::new)
    }

    pub fn registries(&self) -> &Registries {
        &self.source_metadata().registries
    }

    pub fn file_imports(&self) -> &AllFileImports {
        &self.source_metadata().file_imports
    }

    pub fn file_comments(&self) -> &AllFileComments {
        &self.source_metadata().file_comments
    }

    pub fn messages(&self) -> &MessageData {
        self.messages.get_or_init(|| {
            let message_dir = self.resolved_messages_dir();
            let scan_results = scan_message_files(&message_dir).expect("Failed to scan messages");

            let primary_messages = scan_results
                .messages
                .get(&self.config.primary_locale)
                .expect("Primary locale messages not found")
                .clone();

            MessageData {
                all_messages: scan_results.messages,
                primary_messages,
            }
        })
    }

    pub fn all_key_usages(&self) -> &AllKeyUsages {
        &self.resolved_data().key_usages
    }

    pub fn hardcoded_issues(&self) -> &AllHardcodedIssues {
        &self.resolved_data().hardcoded_issues
    }

    pub fn used_keys(&self) -> &HashSet<String> {
        self.used_keys.get_or_init(|| {
            let mut used_keys = HashSet::new();
            let extractions = self.all_key_usages();
            for file_usages in extractions.values() {
                for resolved in &file_usages.resolved {
                    used_keys.insert(resolved.key.as_str().to_string());
                }
            }
            used_keys
        })
    }

    pub fn available_keys(&self) -> HashSet<String> {
        self.messages().primary_messages.keys().cloned().collect()
    }

    pub fn source_metadata(&self) -> &SourceMetadata {
        self.source_metadata.get_or_init(|| {
            let available_keys = self.available_keys();

            let parsed_files = self.parsed_files();

            // Phase 1: Collect registries AND comments (Biome-style: comments collected first)
            let (registries, file_imports, file_comments) =
                collect_registries_and_comments(parsed_files, &available_keys);

            SourceMetadata {
                registries,
                file_imports,
                file_comments,
            }
        })
    }

    fn resolved_data(&self) -> &ResolvedData {
        self.resolved_data.get_or_init(|| {
            let parsed_files = self.parsed_files();

            let available_keys = self.available_keys();

            let registries = self.registries();
            let file_imports = self.file_imports();
            let file_comments = self.file_comments();

            let (key_usages, hardcoded_issues) = extract_from_files(
                &self.files,
                parsed_files,
                registries,
                file_imports,
                file_comments,
                &self.config.checked_attributes,
                &self.ignore_texts,
                &available_keys,
            );

            ResolvedData {
                key_usages,
                hardcoded_issues,
            }
        })
    }

    // ============================================================
    // Utility methods
    // ============================================================

    /// Resolve the messages directory path relative to root_dir.
    pub fn resolved_messages_dir(&self) -> PathBuf {
        let p = Path::new(&self.config.messages_dir);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            // If user runs with `--path .`, keep the original relative path (e.g. "./messages")
            // to avoid noisy paths like "././messages" in output/snapshots.
            let is_cur_dir = self
                .root_dir
                .components()
                .all(|c| matches!(c, Component::CurDir));
            if is_cur_dir {
                p.to_path_buf()
            } else {
                // Strip leading "./" so joins become "<root>/messages" instead of "<root>/./messages".
                let rel = p.strip_prefix(Path::new(".")).unwrap_or(p);
                self.root_dir.join(rel)
            }
        }
    }
}

fn collect_registries_and_comments(
    parsed_files: &HashMap<String, ParsedJSX>,
    _available_keys: &std::collections::HashSet<String>,
) -> (Registries, AllFileImports, AllFileComments) {
    let mut schema = HashMap::new();
    let mut key_object = HashMap::new();
    let mut key_array = HashMap::new();
    let mut string_array = HashMap::new();
    let mut translation_prop = HashMap::new();
    let mut translation_fn_call = HashMap::new();
    let mut default_exports = HashMap::new();
    let mut file_imports: AllFileImports = HashMap::new();
    let mut file_comments: AllFileComments = HashMap::new();
    let mut translation_props_by_file: Vec<(String, Vec<TranslationProp>)> = Vec::new();

    for (file_path, parsed) in parsed_files {
        // Collect registries
        let mut collector = RegistryCollector::new(file_path);
        parsed.module.visit_with(&mut collector);

        // Collect comments (Biome-style: in same phase as registries)
        let comments = CommentCollector::collect(&parsed.comments, &parsed.source_map);
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

/// Phase 2 & 3: Extract and resolve translation keys from all files.
///
/// - Phase 2: Collect raw translation calls and hardcoded issues
/// - Phase 3: Resolve raw calls to ResolvedKeyUsage/UnresolvedKeyUsage
#[allow(clippy::too_many_arguments)]
fn extract_from_files(
    files: &std::collections::HashSet<String>,
    parsed_files: &HashMap<String, ParsedJSX>,
    registries: &Registries,
    file_imports: &AllFileImports,
    file_comments: &AllFileComments,
    checked_attributes: &[String],
    ignore_texts: &std::collections::HashSet<String>,
    available_keys: &std::collections::HashSet<String>,
) -> (AllKeyUsages, AllHardcodedIssues) {
    let mut key_usages = HashMap::new();
    let mut hardcoded_issues = HashMap::new();

    for file_path in files {
        let Some(parsed) = parsed_files.get(file_path) else {
            continue;
        };

        let imports = file_imports.get(file_path).cloned().unwrap_or_default();
        let comments = file_comments
            .get(file_path)
            .expect("comments should be collected in Phase 1");

        // Phase 2: Collect raw translation calls and hardcoded text
        let analyzer = FileAnalyzer::new(
            file_path,
            &parsed.source_map,
            comments,
            checked_attributes,
            ignore_texts,
            registries,
            &imports,
        );
        let result = analyzer.analyze(&parsed.module);

        // Phase 3: Resolve raw calls and schema calls to key usages
        let file_key_usages = resolve_translation_calls(
            &result.raw_calls,
            &result.schema_calls,
            file_path,
            comments,
            registries,
            available_keys,
        );

        key_usages.insert(file_path.clone(), file_key_usages);
        hardcoded_issues.insert(file_path.clone(), result.hardcoded_issues);
    }

    (key_usages, hardcoded_issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a minimal CheckContext for testing without file system dependencies.
    fn create_test_context(root_dir: &str, messages_dir: &str) -> CheckContext {
        CheckContext {
            config: Config {
                messages_dir: messages_dir.to_string(),
                ..Config::default()
            },
            root_dir: PathBuf::from(root_dir),
            files: HashSet::new(),
            ignore_texts: HashSet::new(),
            verbose: false,
            parsed_files: OnceCell::new(),
            parsed_files_errors: OnceCell::new(),
            source_metadata: OnceCell::new(),
            messages: OnceCell::new(),
            used_keys: OnceCell::new(),
            resolved_data: OnceCell::new(),
        }
    }

    #[test]
    fn test_resolved_messages_dir_absolute_path() {
        let ctx = create_test_context("/project", "/absolute/path/messages");
        assert_eq!(
            ctx.resolved_messages_dir(),
            PathBuf::from("/absolute/path/messages")
        );
    }

    #[test]
    fn test_resolved_messages_dir_relative_with_dot() {
        let ctx = create_test_context(".", "./messages");
        assert_eq!(ctx.resolved_messages_dir(), PathBuf::from("./messages"));
    }

    #[test]
    fn test_resolved_messages_dir_relative_with_root() {
        let ctx = create_test_context("/project/app", "./messages");
        assert_eq!(
            ctx.resolved_messages_dir(),
            PathBuf::from("/project/app/messages")
        );
    }

    #[test]
    fn test_resolved_messages_dir_relative_no_dot_prefix() {
        let ctx = create_test_context("/project", "locales");
        assert_eq!(
            ctx.resolved_messages_dir(),
            PathBuf::from("/project/locales")
        );
    }
}

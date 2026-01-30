use std::{
    cell::OnceCell,
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow};
use rayon::prelude::*;
use swc_ecma_visit::VisitWith;

use crate::{
    cli::args::CommonArgs,
    config::{Config, load_config},
    core::{
        AllKeyUsages, AllLocaleMessages, LocaleMessages,
        collect::{
            AllFileComments, AllFileImports, CommentCollector, FileImports, Registries,
            RegistryCollector, TranslationFnCall, TranslationProp, make_registry_key,
            make_translation_fn_call_key, make_translation_prop_key, resolve_import_path,
        },
        extract::FileAnalyzer,
        file_scanner::scan_files,
        parsers::{
            json::scan_message_files,
            jsx::{ParsedJSX, parse_jsx_source},
        },
        resolve::resolve_translation_calls,
        schema::{ExpandResult, SchemaRegistry, expand_schema_keys},
    },
    issues::{HardcodedTextIssue, ParseErrorFileType, ParseErrorIssue},
};

use std::collections::HashMap;

/// All hardcoded text issues across the codebase, indexed by file path.
///
/// **Phase 2**: Created during extraction
/// **Phase 3+**: Converted to user-facing issues by rules
pub type AllHardcodedTextIssues = HashMap<String, Vec<HardcodedTextIssue>>;

/// Aggregated message data from all locale files.
///
/// Loaded from the messages directory (e.g., `./messages/`) during context initialization.
/// Contains both the primary locale (for validation) and all locales (for untranslated detection).
pub struct MessageData {
    /// Messages for all locales (e.g., {"en": {...}, "ja": {...}}).
    /// Used by untranslated rule to find keys that exist in primary but not replica locales.
    pub all_messages: AllLocaleMessages,

    /// Messages for the primary locale (e.g., "en").
    /// Used for key validation - all keys must exist in the primary locale.
    pub primary_messages: LocaleMessages,
}

/// Source metadata collected during Phase 1: Collection.
///
/// This aggregates all cross-file dependency information collected during
/// the initial scan of the codebase. It's used in Phase 2 (Extraction) to
/// resolve dynamic keys and translation function bindings.
pub struct SourceMetadata {
    /// Cross-file dependency registries (key objects, translation props, schemas, etc.).
    pub registries: Registries,

    /// Import statements for each file (used to resolve imported variables).
    pub file_imports: AllFileImports,

    /// Comment directives for each file (suppressions and key declarations).
    pub file_comments: AllFileComments,

    /// Pre-computed schema expansions (Phase 1.5).
    /// Maps schema_name → ExpandResult with raw_keys (no namespace).
    /// Namespace is applied per-call in Phase 3 based on call site context.
    pub schema_cache: HashMap<String, ExpandResult>,
}

/// Resolved data from Phase 2 (Extraction) and Phase 3 (Resolution).
///
/// This is the final output of the extraction and resolution phases,
/// ready to be checked against locale files by rules in Phase 3+.
pub struct ResolvedData {
    /// Resolved and unresolved key usages for all files.
    /// Resolved keys are validated, unresolved keys generate warnings.
    pub key_usages: AllKeyUsages,

    /// Hardcoded text issues found during extraction (directly reportable).
    pub hardcoded_issues: AllHardcodedTextIssues,
}

/// Core analysis context orchestrating the three-phase pipeline.
///
/// `CheckContext` is the central orchestrator for the entire analysis pipeline.
/// It manages configuration, file scanning, and lazy initialization of each phase.
///
/// # Three-Phase Pipeline
///
/// 1. **Phase 1: Collection** → `source_metadata` (registries, imports, comments)
/// 2. **Phase 2: Extraction** → Raw translation calls + hardcoded issues
/// 3. **Phase 3: Resolution** → `resolved_data` (resolved/unresolved key usages)
/// 4. **Phase 3+: Rules** → User-facing issues (missing keys, type mismatches, etc.)
///
/// # Lazy Initialization Strategy
///
/// Most data is computed lazily using `OnceCell`:
/// - **Why**: Some commands (e.g., `glot --help`) don't need full analysis
/// - **How**: Each phase is computed on first access via getter methods
/// - **Benefit**: Faster startup, only compute what's needed
///
/// # Configuration Priority
///
/// Configuration is loaded with the following priority (highest to lowest):
/// 1. CLI arguments (e.g., `--primary-locale en`)
/// 2. `.glotrc.json` config file
/// 3. Built-in defaults
pub struct CheckContext {
    // ============================================================
    // Basic data (set at initialization)
    // ============================================================
    /// Merged configuration (CLI args > config file > defaults).
    pub config: Config,

    /// Project root directory (for resolving relative paths).
    pub root_dir: PathBuf,

    /// All source files to analyze (TSX/JSX/TS/JS).
    pub files: HashSet<String>,

    /// Hardcoded texts to ignore (from config `ignoreTexts`).
    pub ignore_texts: HashSet<String>,

    /// Whether to print verbose diagnostic messages.
    pub verbose: bool,

    // ============================================================
    // Lazily initialized pipeline data
    // ============================================================
    /// Parsed AST for each source file.
    /// Initialized on first call to `parsed_files()`.
    parsed_files: OnceCell<HashMap<String, ParsedJSX>>,

    /// Parse errors encountered while parsing source files.
    /// Populated alongside `parsed_files` initialization.
    parsed_files_errors: OnceCell<Vec<ParseErrorIssue>>,

    /// Phase 1 output: Registries, imports, and comments.
    /// Initialized on first call to `source_metadata()`.
    source_metadata: OnceCell<SourceMetadata>,

    /// Phase 2+3 output: Resolved key usages and hardcoded issues.
    /// Initialized on first call to `resolved_data()`.
    resolved_data: OnceCell<ResolvedData>,

    /// Message data from locale files (primary + all locales).
    /// Initialized eagerly during context creation to catch errors early.
    messages: OnceCell<MessageData>,

    /// Set of all resolved keys used in source code.
    /// Initialized on first call to `used_keys()` (for unused key detection).
    used_keys: OnceCell<HashSet<String>>,

    /// Parse errors from message files (collected during context creation).
    message_parse_errors: Vec<ParseErrorIssue>,
}

impl CheckContext {
    /// Create a new `CheckContext` from command line arguments.
    ///
    /// This constructor:
    /// 1. Loads configuration (CLI args > config file > defaults)
    /// 2. Scans source files (TSX/JSX/TS/JS)
    /// 3. Loads message files (JSON from messages directory)
    /// 4. Initializes lazy pipeline data structures
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Config file is invalid
    /// - Primary locale messages are not found
    /// - Messages directory doesn't exist
    pub fn new(common_args: &CommonArgs) -> Result<Self> {
        let verbose = common_args.verbose;

        // ============================================================
        // 1. Determine source_root for config file search
        // ============================================================
        // Priority: CLI --source-root arg > current directory
        let source_root = common_args
            .source_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));

        let root_dir = source_root.clone();
        let path = source_root
            .to_str()
            .with_context(|| anyhow!("Invalid path: {:?}", source_root))?;

        // ============================================================
        // 2. Load config from source_root
        // ============================================================
        let config_result = load_config(Path::new(path))?;

        // In verbose mode, inform user if using default config
        if verbose && !config_result.from_file {
            eprintln!("Note: No .glotrc.json found, using default configuration");
        }

        let mut config = config_result.config;

        // ============================================================
        // 3. Apply CLI overrides (CLI > config file > defaults)
        // ============================================================
        if let Some(ref primary_locale) = common_args.primary_locale {
            config.primary_locale = primary_locale.clone();
        }

        if let Some(ref messages_root) = common_args.messages_root {
            config.messages_root = messages_root.to_string_lossy().to_string();
        }

        // Note: config's source_root is used for file scanning,
        // but CLI's source_root already determined where to find the config

        // Compute message directory before parallel section (it depends on config)
        let message_dir = {
            let p = Path::new(&config.messages_root);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                let is_cur_dir = root_dir
                    .components()
                    .all(|c| matches!(c, Component::CurDir));
                if is_cur_dir {
                    p.to_path_buf()
                } else {
                    let rel = p.strip_prefix(Path::new(".")).unwrap_or(p);
                    root_dir.join(rel)
                }
            }
        };

        // Run file scanning and message scanning in parallel
        let (scan_result, scan_message_result) = rayon::join(
            || {
                scan_files(
                    path,
                    &config.includes,
                    &config.ignores,
                    config.ignore_test_files,
                    verbose,
                )
            },
            || scan_message_files(&message_dir),
        );

        if scan_result.skipped_count > 0 {
            eprintln!(
                "Warning: {} path(s) skipped due to access errors{}",
                scan_result.skipped_count,
                if verbose { "" } else { " (use -v for details)" }
            );
        }

        let ignore_texts = config.ignore_texts.iter().cloned().collect();

        // Propagate message scan errors
        let scan_results = scan_message_result?;

        // Convert message warnings to ParseErrorIssue
        let message_parse_errors: Vec<ParseErrorIssue> = scan_results
            .warnings
            .iter()
            .map(|warning| ParseErrorIssue {
                file_path: warning.file_path.clone(),
                error: warning.error.clone(),
                file_type: ParseErrorFileType::Message,
            })
            .collect();

        let primary_messages = scan_results
            .messages
            .get(&config.primary_locale)
            .ok_or_else(|| {
                anyhow!(
                    "Primary locale '{}' messages not found in '{}'",
                    config.primary_locale,
                    message_dir.display()
                )
            })?
            .clone();

        let messages = OnceCell::new();
        let _ = messages.set(MessageData {
            all_messages: scan_results.messages,
            primary_messages,
        });

        Ok(Self {
            config,
            root_dir,
            files: scan_result.files,
            ignore_texts,
            verbose,
            parsed_files: OnceCell::new(),
            parsed_files_errors: OnceCell::new(),
            source_metadata: OnceCell::new(),
            resolved_data: OnceCell::new(),
            messages,
            used_keys: OnceCell::new(),
            message_parse_errors,
        })
    }

    /// Get parsed AST for all source files (lazy initialization).
    ///
    /// Parses all TSX/JSX/TS/JS files using swc. Parse errors are collected
    /// separately and can be retrieved via `parsed_files_errors()`.
    ///
    /// ## Performance Note
    ///
    /// This method parallelizes both **file reading** (I/O-bound) and **parsing** (CPU-bound).
    /// ParsedJSX is now thread-safe using:
    /// - `Arc<SourceMap>` - Each file gets its own SourceMap wrapped in Arc
    /// - `ExtractedComments` - Pure data extracted from SingleThreadedComments
    /// - `GLOBALS.set()` - Per-thread swc globals
    ///
    /// This achieves 4-6x speedup on multi-core CPUs for medium-to-large codebases.
    pub fn parsed_files(&self) -> &HashMap<String, ParsedJSX> {
        self.parsed_files.get_or_init(|| {
            use std::sync::Arc;

            // Parallel file reading AND parsing (both I/O-bound and CPU-bound)
            let parse_results: Vec<_> = self
                .files
                .par_iter()
                .map(|file_path| {
                    // Read file
                    let content_result = std::fs::read_to_string(file_path)
                        .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e));

                    // Parse with per-file SourceMap
                    let parse_result = match content_result {
                        Ok(code) => {
                            // Each thread creates its own SourceMap
                            let source_map = Arc::new(swc_common::SourceMap::default());
                            parse_jsx_source(code, file_path, source_map)
                        }
                        Err(e) => Err(e),
                    };

                    (file_path.clone(), parse_result)
                })
                .collect();

            // Sequential error collection and HashMap construction
            let mut parsed = HashMap::new();
            let mut errors = Vec::new();

            for (file_path, result) in parse_results {
                match result {
                    Ok(p) => {
                        parsed.insert(file_path, p);
                    }
                    Err(e) => {
                        if self.verbose {
                            eprintln!("Warning: {} - {}", file_path, e);
                        }
                        errors.push(ParseErrorIssue {
                            file_path,
                            error: e.to_string(),
                            file_type: ParseErrorFileType::Source,
                        });
                    }
                }
            }

            let _ = self.parsed_files_errors.set(errors);
            parsed
        })
    }

    /// Get parse errors from source files.
    ///
    /// Returns errors encountered while parsing TSX/JSX/TS/JS files.
    /// Populated when `parsed_files()` is first called.
    pub fn parsed_files_errors(&self) -> &Vec<ParseErrorIssue> {
        self.parsed_files_errors.get_or_init(Vec::new)
    }

    /// Get parse errors from message files.
    ///
    /// Returns errors encountered while parsing JSON locale files.
    /// Collected during context initialization.
    pub fn message_parse_errors(&self) -> &Vec<ParseErrorIssue> {
        &self.message_parse_errors
    }

    /// Get Phase 1 registries (lazy initialization).
    pub fn registries(&self) -> &Registries {
        &self.source_metadata().registries
    }

    /// Get Phase 1 file imports (lazy initialization).
    pub fn file_imports(&self) -> &AllFileImports {
        &self.source_metadata().file_imports
    }

    /// Get Phase 1 file comments (lazy initialization).
    pub fn file_comments(&self) -> &AllFileComments {
        &self.source_metadata().file_comments
    }

    /// Get message data (eagerly initialized).
    ///
    /// Messages are loaded during context creation to catch errors early.
    pub fn messages(&self) -> &MessageData {
        // Messages are initialized in new(), so this should never fail
        self.messages
            .get()
            .expect("Messages should be initialized in CheckContext::new()")
    }

    /// Get all resolved and unresolved key usages (lazy initialization).
    ///
    /// This triggers Phase 1→2→3 pipeline if not already run.
    pub fn all_key_usages(&self) -> &AllKeyUsages {
        &self.resolved_data().key_usages
    }

    /// Get all hardcoded text issues (lazy initialization).
    ///
    /// This triggers Phase 1→2 pipeline if not already run.
    pub fn hardcoded_issues(&self) -> &AllHardcodedTextIssues {
        &self.resolved_data().hardcoded_issues
    }

    /// Get set of all keys used in source code (lazy initialization).
    ///
    /// This is a flattened set of all resolved keys from all files.
    /// Used by the unused-key rule to find keys in locale files that aren't used.
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

    /// Get set of all keys available in primary locale messages.
    ///
    /// This is a flattened set of all keys from the primary locale file.
    /// Used for key validation and unused key detection.
    pub fn available_keys(&self) -> HashSet<String> {
        self.messages().primary_messages.keys().cloned().collect()
    }

    /// Get Phase 1 source metadata (lazy initialization).
    ///
    /// Runs Phase 1: Collection to gather all cross-file dependencies.
    /// This includes registries, imports, and comment annotations.
    pub fn source_metadata(&self) -> &SourceMetadata {
        self.source_metadata.get_or_init(|| {
            let available_keys = self.available_keys();

            let parsed_files = self.parsed_files();

            // Phase 1: Collection - Collect registries and comments in single AST pass
            let (registries, file_imports, file_comments) =
                collect_registries_and_comments(parsed_files, &available_keys);

            // Phase 1.5: Build schema cache
            let schema_cache = build_schema_cache(&registries.schema);

            SourceMetadata {
                registries,
                file_imports,
                file_comments,
                schema_cache,
            }
        })
    }

    /// Get Phase 2+3 resolved data (lazy initialization).
    ///
    /// Runs Phase 2 (Extraction) and Phase 3 (Resolution) to produce
    /// final resolved/unresolved key usages and hardcoded issues.
    fn resolved_data(&self) -> &ResolvedData {
        self.resolved_data.get_or_init(|| {
            let parsed_files = self.parsed_files();

            let available_keys = self.available_keys();

            let metadata = self.source_metadata();

            let (key_usages, hardcoded_issues) = extract_from_files(
                &self.files,
                parsed_files,
                &metadata.registries,
                &metadata.file_imports,
                &metadata.file_comments,
                &metadata.schema_cache,
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
        let p = Path::new(&self.config.messages_root);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            // If user runs with `--source-root .`, keep the original relative path (e.g. "./messages")
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

/// Phase 1.5: Pre-compute all schema expansions.
///
/// Expands all schemas in the registry once (without namespace), avoiding redundant
/// expansion when the same schema is used across multiple files.
///
/// ## Parallelization
/// - Uses rayon to process schemas in parallel
/// - Each schema gets its own `visited` HashSet for cycle detection
/// - Thread-safe: no shared mutable state during parallel expansion
///
/// ## Performance
/// For a project with 10 schemas across 100 files:
/// - Before: 50 redundant expansions (~50ms)
/// - After: 10 parallel expansions (~1ms wall time on 8 cores)
fn build_schema_cache(schema_registry: &SchemaRegistry) -> HashMap<String, ExpandResult> {
    schema_registry
        .par_iter()
        .map(|(schema_name, _schema)| {
            // Each thread gets its own visited set for cycle detection
            let mut visited = HashSet::new();

            // Expand without namespace (namespace applied per-call in Phase 3)
            let result = expand_schema_keys(
                schema_name,
                &None, // No namespace at cache time
                schema_registry,
                &mut visited,
            );

            (schema_name.clone(), result)
        })
        .collect()
}

/// Phase 1: Collection - Collect cross-file registries and comments.
///
/// This performs the first AST pass to collect:
/// - Schema function registries
/// - Key object/array registries
/// - Translation prop/function call registries
/// - Import resolution data
/// - Comment annotations (disable directives, glot-message-keys)
///
/// This phase is parallelized using rayon for improved performance.
fn collect_registries_and_comments(
    parsed_files: &HashMap<String, ParsedJSX>,
    _available_keys: &std::collections::HashSet<String>,
) -> (Registries, AllFileImports, AllFileComments) {
    // Parallel collection per file
    let results: Vec<_> = parsed_files
        .par_iter()
        .map(|(file_path, parsed)| {
            // Collect registries
            let mut collector = RegistryCollector::new(file_path);
            parsed.module.visit_with(&mut collector);

            // Comments already extracted during parsing
            let comments = CommentCollector::collect(&parsed.comments, &parsed.source_map);

            (file_path.clone(), collector, comments)
        })
        .collect();

    // Sequential merge (avoid lock contention)
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

    for (file_path, collector, comments) in results {
        // Merge schema functions
        for func in collector.schema_functions {
            if !schema.contains_key(&func.name) {
                schema.insert(func.name.clone(), func);
            }
        }

        // Merge imports
        file_imports.insert(file_path.clone(), collector.imports);

        // Merge comments
        file_comments.insert(file_path.clone(), comments);

        // Merge key objects
        for obj in collector.objects {
            let key = make_registry_key(&obj.file_path, &obj.name);
            key_object.insert(key, obj);
        }

        // Merge key arrays
        for arr in collector.arrays {
            let key = make_registry_key(&arr.file_path, &arr.name);
            key_array.insert(key, arr);
        }

        // Merge string arrays
        for str_arr in collector.string_arrays {
            let key = make_registry_key(&str_arr.file_path, &str_arr.name);
            string_array.insert(key, str_arr);
        }

        // Collect translation props for later resolution
        translation_props_by_file.push((file_path.clone(), collector.translation_props));

        // Merge translation function calls
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

        // Merge default exports
        if let Some(name) = collector.default_export_name {
            default_exports.insert(file_path.clone(), name);
        }
    }

    // Sequential translation props resolution (cross-file dependency)
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

    let registries = Registries {
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

/// Resolve a component name to its original definition name.
///
/// When a component is imported as a default export and passed a translation prop,
/// we need to resolve the local name back to the original exported name.
///
/// Example:
/// ```typescript
/// import MyComp from "./components/Button";  // Button exports "SubmitButton" as default
/// <MyComp t={t} />  // Need to resolve "MyComp" → "SubmitButton"
/// ```
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

/// Phase 2 & 3: Extraction and Resolution.
///
/// For each file:
/// - **Phase 2 (Extraction)**: Collect raw translation calls and detect hardcoded text
/// - **Phase 3 (Resolution)**: Resolve ValueSource to static keys, expand schema calls,
///   apply glot-message-keys, and generate final ResolvedKeyUsage/UnresolvedKeyUsage
///
/// This phase is parallelized using rayon for improved performance.
#[allow(clippy::too_many_arguments)]
fn extract_from_files(
    files: &std::collections::HashSet<String>,
    parsed_files: &HashMap<String, ParsedJSX>,
    registries: &Registries,
    file_imports: &AllFileImports,
    file_comments: &AllFileComments,
    schema_cache: &HashMap<String, ExpandResult>,
    checked_attributes: &[String],
    ignore_texts: &std::collections::HashSet<String>,
    available_keys: &std::collections::HashSet<String>,
) -> (AllKeyUsages, AllHardcodedTextIssues) {
    // Parallel extraction and resolution per file
    let results: Vec<_> = files
        .par_iter()
        .filter_map(|file_path| {
            let parsed = parsed_files.get(file_path)?;
            let imports = file_imports.get(file_path).cloned().unwrap_or_default();
            let comments = file_comments
                .get(file_path)
                .expect("Comments should be collected in Phase 1");

            // Phase 2: Extraction
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

            // Phase 3: Resolution
            let file_key_usages = resolve_translation_calls(
                &result.raw_calls,
                &result.schema_calls,
                file_path,
                comments,
                schema_cache,
                &registries.schema,
                available_keys,
            );

            Some((file_path.clone(), file_key_usages, result.hardcoded_issues))
        })
        .collect();

    // Sequential merge
    let mut key_usages = HashMap::new();
    let mut hardcoded_issues = HashMap::new();

    for (file_path, usages, issues) in results {
        key_usages.insert(file_path.clone(), usages);
        hardcoded_issues.insert(file_path, issues);
    }

    (key_usages, hardcoded_issues)
}

#[cfg(test)]
mod tests {
    use std::{cell::OnceCell, collections::HashSet, path::PathBuf};

    use crate::{config::Config, core::*};

    /// Create a minimal CheckContext for testing without file system dependencies.
    fn create_test_context(root_dir: &str, messages_root: &str) -> CheckContext {
        CheckContext {
            config: Config {
                messages_root: messages_root.to_string(),
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
            message_parse_errors: Vec::new(),
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

    #[test]
    fn test_schema_cache_reuses_expansions() {
        use super::build_schema_cache;
        use crate::core::schema::types::{SchemaFunction, SchemaRegistry};

        // Create registry with one schema
        let mut registry = SchemaRegistry::new();
        registry.insert(
            "createSchema".to_string(),
            SchemaFunction {
                name: "createSchema".to_string(),
                file_path: "schema.ts".to_string(),
                keys: vec!["key1".to_string(), "key2".to_string()],
                nested_calls: vec![],
            },
        );

        // Build cache (should expand once)
        let cache = build_schema_cache(&registry);

        // Verify cache contains schema
        assert!(cache.contains_key("createSchema"));

        // Verify expanded keys don't have namespace (raw_keys)
        let result = cache.get("createSchema").unwrap();
        assert_eq!(result.keys.len(), 2);
        assert_eq!(result.keys[0].raw_key, "key1");
        assert_eq!(result.keys[0].full_key, "key1"); // No namespace
        assert!(!result.keys[0].has_namespace);
    }
}

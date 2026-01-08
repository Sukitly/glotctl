use std::{
    cell::OnceCell,
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow};

use crate::{
    args::CommonArgs,
    checkers::key_objects::{KeyArrayRegistry, KeyObjectRegistry, StringArrayRegistry},
    checkers::missing_keys::MissingKeyResult,
    checkers::schema::SchemaRegistry,
    commands::shared,
    config::Config,
    config::load_config,
    file_scanner::scan_files,
    issue::Issue,
    parsers::json::MessageMap,
    parsers::json::scan_message_files,
};

use std::collections::HashMap;

/// Type alias for all file imports across the codebase.
pub type AllFileImports = HashMap<String, crate::checkers::key_objects::FileImports>;

/// Type alias for all extraction results (one per file).
pub type AllExtractions = HashMap<String, MissingKeyResult>;

/// Registry of parsed symbol information (schemas, objects, arrays).
/// Does NOT contain file_imports - that's stored separately.
pub struct Registries {
    pub schema: SchemaRegistry,
    pub key_object: KeyObjectRegistry,
    pub key_array: KeyArrayRegistry,
    pub string_array: StringArrayRegistry,
}

/// Aggregated message data from all locale files.
pub struct MessageData {
    pub all_messages: HashMap<String, MessageMap>,
    pub primary_messages: Option<MessageMap>,
}

/// Pure data container for check operations.
///
/// # Responsibility Separation
///
/// This struct follows a clear separation of concerns:
/// - **CheckContext**: Data container - holds configuration and cached data
/// - **CheckRunner/CleanRunner**: Orchestrators - load data and coordinate checks
/// - **shared module**: Data loading - build_registries, build_extractions, etc.
///
/// # Usage Pattern
///
/// 1. Create with `CheckContext::new(&args)`
/// 2. Runner calls `shared::build_*` functions to load data
/// 3. Runner calls `ctx.set_*` to store data in OnceCell fields
/// 4. Runner calls `ctx.get_*` to retrieve data for checks
///
/// Data is loaded lazily and cached - once set, it cannot be changed.
pub struct CheckContext {
    // Basic data (set at initialization)
    pub config: Config,
    pub root_dir: PathBuf,
    pub files: HashSet<String>,
    pub ignore_texts: HashSet<String>,
    pub verbose: bool,

    // Lazy-loaded data (set by CheckRunner when needed)
    registries: OnceCell<Registries>,
    file_imports: OnceCell<AllFileImports>,
    messages: OnceCell<MessageData>,
    extractions: OnceCell<AllExtractions>,
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
            registries: OnceCell::new(),
            file_imports: OnceCell::new(),
            messages: OnceCell::new(),
            extractions: OnceCell::new(),
            used_keys: OnceCell::new(),
        })
    }

    // ============================================================
    // Getters - return Option to indicate whether data is loaded
    // ============================================================

    pub fn registries(&self) -> Option<&Registries> {
        self.registries.get()
    }

    pub fn file_imports(&self) -> Option<&AllFileImports> {
        self.file_imports.get()
    }

    pub fn messages(&self) -> Option<&MessageData> {
        self.messages.get()
    }

    pub fn extractions(&self) -> Option<&AllExtractions> {
        self.extractions.get()
    }

    pub fn used_keys(&self) -> Option<&HashSet<String>> {
        self.used_keys.get()
    }

    // ============================================================
    // Setters - called by CheckRunner to populate data
    // ============================================================

    pub fn set_registries(&self, data: Registries) {
        let result = self.registries.set(data);
        debug_assert!(result.is_ok(), "registries already initialized");
    }

    pub fn set_file_imports(&self, data: AllFileImports) {
        let result = self.file_imports.set(data);
        debug_assert!(result.is_ok(), "file_imports already initialized");
    }

    pub fn set_messages(&self, data: MessageData) {
        let result = self.messages.set(data);
        debug_assert!(result.is_ok(), "messages already initialized");
    }

    pub fn set_extractions(&self, data: AllExtractions) {
        let result = self.extractions.set(data);
        debug_assert!(result.is_ok(), "extractions already initialized");
    }

    pub fn set_used_keys(&self, data: HashSet<String>) {
        let result = self.used_keys.set(data);
        debug_assert!(result.is_ok(), "used_keys already initialized");
    }

    // ============================================================
    // Data Loading Logic (Self-Populating)
    // ============================================================

    /// Ensure registries and file_imports are loaded.
    ///
    /// Returns parse errors as Issues (if any files failed to parse).
    pub fn ensure_registries(&self) -> Result<Vec<Issue>> {
        // Check both registries and file_imports together (they are always set as a pair)
        if self.registries.get().is_some() && self.file_imports.get().is_some() {
            return Ok(Vec::new());
        }

        let (registries, file_imports, errors) = shared::build_registries(self);
        self.set_registries(registries);
        self.set_file_imports(file_imports);
        Ok(errors)
    }

    /// Ensure messages are loaded.
    pub fn ensure_messages(&self) -> Result<()> {
        if self.messages.get().is_some() {
            return Ok(());
        }

        let message_dir = self.resolved_messages_dir();
        let scan_results = scan_message_files(&message_dir)?;

        let primary_messages = scan_results
            .messages
            .get(&self.config.primary_locale)
            .cloned();

        let data = MessageData {
            all_messages: scan_results.messages,
            primary_messages,
        };

        self.set_messages(data);
        Ok(())
    }

    /// Ensure extractions are loaded for all files.
    /// Returns parse errors as Issues (if any files failed to parse).
    pub fn ensure_extractions(&self) -> Result<Vec<Issue>> {
        if self.extractions.get().is_some() {
            return Ok(Vec::new());
        }

        // extractions depends on registries and messages
        self.ensure_registries()?;
        self.ensure_messages()?;

        let (extractions, errors) = shared::build_extractions(self);
        self.set_extractions(extractions);
        Ok(errors)
    }

    /// Ensure used_keys are collected.
    /// Returns parse errors from ensure_extractions (if any files failed to parse).
    pub fn ensure_used_keys(&self) -> Result<Vec<Issue>> {
        if self.used_keys.get().is_some() {
            return Ok(Vec::new());
        }

        // used_keys depends on extractions
        let parse_errors = self.ensure_extractions()?;

        let used_keys = shared::collect_used_keys(self);
        self.set_used_keys(used_keys);
        Ok(parse_errors)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
            registries: OnceCell::new(),
            file_imports: OnceCell::new(),
            messages: OnceCell::new(),
            extractions: OnceCell::new(),
            used_keys: OnceCell::new(),
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
    fn test_oncecell_getters_return_none_initially() {
        let ctx = create_test_context(".", "./messages");
        assert!(ctx.registries().is_none());
        assert!(ctx.file_imports().is_none());
        assert!(ctx.messages().is_none());
        assert!(ctx.extractions().is_none());
        assert!(ctx.used_keys().is_none());
    }

    #[test]
    fn test_oncecell_setters_and_getters() {
        let ctx = create_test_context(".", "./messages");

        // Set and get file_imports
        let file_imports: AllFileImports = HashMap::new();
        ctx.set_file_imports(file_imports);
        assert!(ctx.file_imports().is_some());
        assert!(ctx.file_imports().unwrap().is_empty());

        // Set and get used_keys
        let mut used_keys = HashSet::new();
        used_keys.insert("test.key".to_string());
        ctx.set_used_keys(used_keys);
        assert!(ctx.used_keys().is_some());
        assert!(ctx.used_keys().unwrap().contains("test.key"));
    }

    #[test]
    fn test_oncecell_messages_data() {
        let ctx = create_test_context(".", "./messages");

        let message_data = MessageData {
            all_messages: HashMap::new(),
            primary_messages: None,
        };
        ctx.set_messages(message_data);

        assert!(ctx.messages().is_some());
        let messages = ctx.messages().unwrap();
        assert!(messages.all_messages.is_empty());
        assert!(messages.primary_messages.is_none());
    }
}

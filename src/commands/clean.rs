use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Result, bail};
use colored::Colorize;

use crate::{
    RunResult,
    args::CleanArgs,
    commands::{
        check::{find_orphan_keys, find_unused_keys},
        context::{CheckContext, MessageData},
        shared,
    },
    extraction::UnresolvedKeyReason,
    issue::{
        Issue, IssueReport, MessageLocation, ParseErrorIssue, Rule, SourceLocation,
        UnresolvedKeyIssue, UnusedKeyIssue,
    },
    json_editor::JsonEditor,
    parsers::json::scan_message_files,
    reporter::{FAILURE_MARK, SUCCESS_MARK},
};

/// Runner for the clean command.
///
/// CleanRunner identifies and optionally removes unused translation keys
/// from message JSON files.
pub struct CleanRunner {
    ctx: CheckContext,
    apply: bool,
    clean_unused: bool,
    clean_orphan: bool,
}

impl CleanRunner {
    pub fn new(args: CleanArgs) -> Result<Self> {
        let ctx = CheckContext::new(&args.common)?;

        // If neither flag is specified, clean both
        let (clean_unused, clean_orphan) = if !args.unused && !args.orphan {
            (true, true)
        } else {
            (args.unused, args.orphan)
        };

        Ok(Self {
            ctx,
            apply: args.apply,
            clean_unused,
            clean_orphan,
        })
    }

    pub fn run(self) -> Result<RunResult> {
        // Step 1: Collect all issues (reusing check logic)
        let all_issues = self.collect_all_issues()?;

        // Step 2: Validate safety - refuse if DynamicKey or ParseError exists
        self.validate_safety(&all_issues)?;

        // Step 3: Filter to cleanable issues
        let cleanable = self.filter_cleanable_issues(&all_issues);

        if cleanable.is_empty() {
            println!("{} {}", SUCCESS_MARK.green(), "No keys to clean.".green());
            return Ok(RunResult {
                error_count: 0,
                warning_count: 0,
                exit_on_errors: true,
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            });
        }

        // Step 4: Group by file path
        let grouped = self.group_by_file(&cleanable);

        // Step 5: Execute (dry-run or apply)
        let result = self.execute_clean(grouped)?;

        Ok(result)
    }

    /// Collect all issues by running the full check pipeline.
    fn collect_all_issues(&self) -> Result<Vec<Issue>> {
        // Parse all source files first
        let mut issues: Vec<Issue> = self.ctx.ensure_parsed_files();

        // Load registries
        let (registries, file_imports) = shared::build_registries(&self.ctx);
        self.ctx.set_registries(registries);
        self.ctx.set_file_imports(file_imports);

        // Load messages
        let (messages, message_warnings) = self.load_messages()?;

        if messages.primary_messages.is_none() {
            let available: Vec<_> = messages.all_messages.keys().collect();
            let hint = if available.is_empty() {
                "No locale files found in the directory.".to_string()
            } else {
                format!("Available locales: {:?}", available)
            };
            bail!(
                "Primary locale file '{}.json' not found in '{}'.\n\
                 {}\n\
                 Hint: Check your .glotrc.json 'primaryLocale' setting.",
                self.ctx.config.primary_locale,
                self.ctx.resolved_messages_dir().display(),
                hint
            );
        }

        self.ctx.set_messages(messages);

        // Convert JSON parse warnings to ParseError issues
        issues.extend(message_warnings.into_iter().map(|warning| {
            Issue::ParseError(ParseErrorIssue {
                file_path: "messages".to_string(),
                error: warning,
            })
        }));

        // Build extractions (uses cached parsed files via FileAnalyzer)
        self.ctx.ensure_extractions()?;

        // Collect used keys
        let used_keys = shared::collect_used_keys(&self.ctx);
        self.ctx.set_used_keys(used_keys);

        // Collect unresolved key issues
        let extractions = self.ctx.extractions().unwrap();
        for file_usages in extractions.values() {
            for unresolved in &file_usages.unresolved {
                // Skip UnknownNamespace - those become UntrackedNamespace issues
                if matches!(
                    unresolved.reason,
                    UnresolvedKeyReason::UnknownNamespace { .. }
                ) {
                    continue;
                }
                issues.push(Issue::UnresolvedKey(UnresolvedKeyIssue {
                    location: SourceLocation::new(
                        &unresolved.context.location.file_path,
                        unresolved.context.location.line,
                    )
                    .with_col(unresolved.context.location.col),
                    reason: unresolved.reason.clone(),
                    source_line: Some(unresolved.context.source_line.clone()),
                    hint: unresolved.hint.clone(),
                    pattern: unresolved.pattern.clone(),
                }));
            }
        }

        // Collect unused keys from primary locale
        let messages = self.ctx.messages().unwrap();
        let used_keys = self.ctx.used_keys().unwrap();

        let unused_keys: HashSet<String> =
            if let Some(primary_messages) = &messages.primary_messages {
                let unused_issues = find_unused_keys(used_keys, primary_messages);
                let keys: HashSet<String> = unused_issues
                    .iter()
                    .map(|i| i.message().to_string())
                    .collect();
                issues.extend(unused_issues);
                keys
            } else {
                HashSet::new()
            };

        // Propagate unused keys to other locales
        // If a key is unused in primary, it should also be removed from all other locales
        for (locale, locale_messages) in &messages.all_messages {
            if locale == &self.ctx.config.primary_locale {
                continue; // Already handled above
            }

            for key in &unused_keys {
                if let Some(entry) = locale_messages.get(key) {
                    issues.push(Issue::UnusedKey(UnusedKeyIssue {
                        location: MessageLocation::new(&entry.file_path, entry.line),
                        key: key.clone(),
                        value: entry.value.clone(),
                    }));
                }
            }
        }

        // Collect orphan keys (keys in other locales that don't exist in primary)
        issues.extend(find_orphan_keys(
            &self.ctx.config.primary_locale,
            &messages.all_messages,
        ));

        Ok(issues)
    }

    /// Validate that there are no blocking issues.
    ///
    /// # Arguments
    /// * `issues` - All collected issues
    fn validate_safety(&self, issues: &[Issue]) -> Result<()> {
        let unresolved_key_count = issues
            .iter()
            .filter(|i| i.rule() == Rule::UnresolvedKey)
            .count();
        let parse_error_count = issues
            .iter()
            .filter(|i| i.rule() == Rule::ParseError)
            .count();

        if unresolved_key_count > 0 {
            bail!(
                "{} {}, {} unresolved key warning(s) found.\n\
                 Unresolved keys prevent tracking all key usage.\n\
                 Run `glot check` to see details, then fix or suppress them.",
                FAILURE_MARK,
                "Cannot clean".red().bold(),
                unresolved_key_count
            );
        }

        if parse_error_count > 0 {
            bail!(
                "{} {}, {} file(s) could not be parsed.\n\
                 Parse errors mean some files could not be analyzed.\n\
                 Run `glot check -v` to see details and fix them.",
                FAILURE_MARK,
                "Cannot clean".red().bold(),
                parse_error_count
            );
        }

        Ok(())
    }

    /// Filter issues to only those that can be cleaned.
    fn filter_cleanable_issues(&self, issues: &[Issue]) -> Vec<Issue> {
        issues
            .iter()
            .filter(|issue| {
                (self.clean_unused && issue.rule() == Rule::UnusedKey)
                    || (self.clean_orphan && issue.rule() == Rule::OrphanKey)
            })
            .cloned()
            .collect()
    }

    /// Group issues by their file path.
    fn group_by_file(&self, issues: &[Issue]) -> HashMap<String, Vec<Issue>> {
        let mut grouped: HashMap<String, Vec<Issue>> = HashMap::new();

        for issue in issues {
            if let Some(file_path) = issue.file_path() {
                grouped
                    .entry(file_path.to_string())
                    .or_default()
                    .push(issue.clone());
            }
        }

        grouped
    }

    /// Execute the clean operation.
    fn execute_clean(&self, grouped: HashMap<String, Vec<Issue>>) -> Result<RunResult> {
        let file_count = grouped.len();

        // Count unused and orphan keys separately
        let mut unused_count = 0;
        let mut orphan_count = 0;
        for issues in grouped.values() {
            for issue in issues {
                match issue.rule() {
                    Rule::UnusedKey => unused_count += 1,
                    Rule::OrphanKey => orphan_count += 1,
                    _ => {}
                }
            }
        }
        let total_keys = unused_count + orphan_count;

        // Sort file paths for deterministic output
        let mut sorted_paths: Vec<_> = grouped.keys().collect();
        sorted_paths.sort();

        if self.apply {
            // Actually delete keys
            for file_path in &sorted_paths {
                let issues = grouped.get(*file_path).unwrap();
                self.print_file_keys(file_path, issues);

                // Edit the file
                let mut editor = JsonEditor::open(Path::new(file_path))?;
                let key_paths: Vec<&str> = issues.iter().map(|i| i.message()).collect();
                editor.delete_keys(&key_paths)?;
                editor.save()?;
            }

            println!(
                "{} {} unused key(s) and {} orphan key(s) from {} file(s).",
                "Deleted".green().bold(),
                unused_count,
                orphan_count,
                file_count
            );

            Ok(RunResult {
                error_count: 0,
                warning_count: 0,
                exit_on_errors: true,
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            })
        } else {
            // Dry-run: just print what would be deleted
            for file_path in &sorted_paths {
                let issues = grouped.get(*file_path).unwrap();
                self.print_file_keys(file_path, issues);
            }

            println!(
                "{} {} unused key(s) and {} orphan key(s) from {} file(s).",
                "Would delete".yellow().bold(),
                unused_count,
                orphan_count,
                file_count
            );
            println!("Run with {} to delete these keys.", "--apply".cyan());

            // Dry-run: report count but don't exit with error code
            Ok(RunResult {
                error_count: total_keys,
                warning_count: 0,
                exit_on_errors: false, // dry-run should not fail CI
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            })
        }
    }

    /// Print keys for a file.
    fn print_file_keys(&self, file_path: &str, issues: &[Issue]) {
        println!("{}:", file_path.blue());
        for issue in issues {
            let line_info = issue
                .line()
                .map(|l| format!(" (line {})", l))
                .unwrap_or_default();

            let value_info = issue
                .format_details()
                .map(|d| format!(": {}", d))
                .unwrap_or_default();

            let rule_tag = match issue.rule() {
                Rule::UnusedKey => "[unused]".dimmed(),
                Rule::OrphanKey => "[orphan]".dimmed(),
                _ => "".normal(),
            };

            println!(
                "  {} {}{}{}  {}",
                "-".dimmed(),
                issue.message(),
                line_info.dimmed(),
                value_info.dimmed(),
                rule_tag
            );
        }
        println!();
    }

    fn load_messages(&self) -> Result<(MessageData, Vec<String>)> {
        let message_dir = self.ctx.resolved_messages_dir();
        let scan_results = scan_message_files(&message_dir)?;

        let primary_messages = scan_results
            .messages
            .get(&self.ctx.config.primary_locale)
            .cloned();

        Ok((
            MessageData {
                all_messages: scan_results.messages,
                primary_messages,
            },
            scan_results.warnings,
        ))
    }
}

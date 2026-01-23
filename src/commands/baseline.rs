use std::collections::{HashMap, HashSet};
use std::fs;

use anyhow::Result;
use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use crate::{
    args::BaselineArgs,
    commands::context::CheckContext,
    directives::DisableRule,
    issue::{HardcodedIssue, Issue},
    reporter::SUCCESS_MARK,
    rules::{untranslated::UntranslatedRule, Checker},
    RunResult,
};

/// Represents a location where a disable comment should be inserted.
#[derive(Debug, Clone)]
struct InsertionTarget {
    file_path: String,
    line: usize,
    col: usize,
    in_jsx_context: bool,
    rules: HashSet<DisableRule>,
}

/// Statistics for the insertion operation.
#[derive(Debug, Default)]
struct InsertionStats {
    /// Number of comments inserted for hardcoded rule
    hardcoded: usize,
    /// Number of comments inserted for untranslated rule
    untranslated: usize,
    /// Number of unique untranslated keys processed
    untranslated_keys: usize,
}

/// Result of collecting insertion targets.
/// Contains: (targets grouped by file, stats, skipped hardcoded issues)
type CollectResult = (
    HashMap<String, Vec<InsertionTarget>>,
    InsertionStats,
    Vec<HardcodedIssue>,
);

/// Represents a comment insertion operation (used during file modification).
#[derive(Debug, Clone)]
struct CommentInsertion {
    /// 1-based line number where comment goes (above the issue line)
    line: usize,
    /// The comment text to insert
    comment: String,
    /// Whitespace prefix to match source indentation
    indentation: String,
}

/// Runner for the baseline command.
///
/// BaselineRunner identifies hardcoded text and untranslated keys, then optionally
/// inserts `glot-disable-next-line` comments to suppress them.
pub struct BaselineRunner {
    ctx: CheckContext,
    apply: bool,
    rules: HashSet<DisableRule>,
}

impl BaselineRunner {
    pub fn new(args: BaselineArgs) -> Result<Self> {
        let ctx = CheckContext::new(&args.common)?;

        // If no rules specified, process all rules
        let rules = if args.rule.is_empty() {
            DisableRule::all()
        } else {
            args.rule.into_iter().collect()
        };

        Ok(Self {
            ctx,
            apply: args.apply,
            rules,
        })
    }

    pub fn run(self) -> Result<RunResult> {
        // Step 1: Collect all insertion targets and stats
        let (targets, stats, skipped_hardcoded) = self.collect_targets()?;

        if targets.is_empty() && skipped_hardcoded.is_empty() {
            println!(
                "{} {}",
                SUCCESS_MARK.green(),
                "No issues found to baseline.".green()
            );
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

        // Step 2: Execute (dry-run or apply)
        self.execute(targets, stats, skipped_hardcoded)
    }

    /// Collects all insertion targets from hardcoded and untranslated issues.
    fn collect_targets(&self) -> Result<CollectResult> {
        // Map from (file_path, line) to InsertionTarget for merging rules on same line
        let mut targets_map: HashMap<(String, usize), InsertionTarget> = HashMap::new();
        let mut skipped_hardcoded: Vec<HardcodedIssue> = Vec::new();
        // Track unique untranslated keys for statistics
        let mut untranslated_keys: HashSet<String> = HashSet::new();

        // Ensure all files are parsed (caches AST for reuse)
        self.ctx.ensure_parsed_files();

        // Collect hardcoded issues (if enabled)
        if self.rules.contains(&DisableRule::Hardcoded) {
            // Ensure file analysis (extractions + hardcoded) is loaded
            self.ctx.ensure_extractions()?;
            self.ctx.ensure_hardcoded_issues()?;
            let extractions = self.ctx.extractions().expect("extractions must be loaded");
            let all_hardcoded_issues = self
                .ctx
                .hardcoded_issues()
                .expect("hardcoded_issues must be loaded");

            // Build translation_lines map from extractions
            let mut translation_lines: HashMap<String, HashSet<usize>> = HashMap::new();
            for (file_path, extraction_result) in extractions {
                let lines: HashSet<usize> =
                    extraction_result.used_keys.iter().map(|k| k.line).collect();
                if !lines.is_empty() {
                    translation_lines.insert(file_path.clone(), lines);
                }
            }

            // Process hardcoded issues from cached results
            for (file_path, issues) in all_hardcoded_issues {
                let file_translation_lines = translation_lines.get(file_path);
                for issue in issues {
                    let has_translation = file_translation_lines
                        .is_some_and(|lines| lines.contains(&issue.location.line));
                    if has_translation {
                        skipped_hardcoded.push(issue.clone());
                    } else {
                        let key = (issue.location.file_path.clone(), issue.location.line);
                        targets_map
                            .entry(key.clone())
                            .or_insert_with(|| InsertionTarget {
                                file_path: issue.location.file_path.clone(),
                                line: issue.location.line,
                                col: issue.location.col.unwrap_or(1),
                                in_jsx_context: issue.location.in_jsx_context,
                                rules: HashSet::new(),
                            })
                            .rules
                            .insert(DisableRule::Hardcoded);
                    }
                }
            }
        }

        // Collect untranslated issues (if enabled)
        if self.rules.contains(&DisableRule::Untranslated) {
            let rule = UntranslatedRule;
            let issues = rule.check(&self.ctx)?;

            for issue in issues {
                if let Issue::Untranslated(ui) = issue {
                    // Track unique keys for statistics
                    untranslated_keys.insert(ui.key.clone());

                    // Add each usage location as an insertion target
                    for usage in &ui.usages {
                        let key = (usage.file_path().to_string(), usage.line());
                        targets_map
                            .entry(key.clone())
                            .or_insert_with(|| InsertionTarget {
                                file_path: usage.file_path().to_string(),
                                line: usage.line(),
                                col: usage.col(),
                                in_jsx_context: usage.in_jsx_context(),
                                rules: HashSet::new(),
                            })
                            .rules
                            .insert(DisableRule::Untranslated);
                    }
                }
            }
        }

        // Sort skipped for deterministic output
        skipped_hardcoded.sort_by(|a, b| {
            (&a.location.file_path, a.location.line).cmp(&(&b.location.file_path, b.location.line))
        });

        // Build statistics from actual targets (not from issue/usage counts)
        let mut stats = InsertionStats {
            untranslated_keys: untranslated_keys.len(),
            ..Default::default()
        };
        for target in targets_map.values() {
            if target.rules.contains(&DisableRule::Hardcoded) {
                stats.hardcoded += 1;
            }
            if target.rules.contains(&DisableRule::Untranslated) {
                stats.untranslated += 1;
            }
        }

        // Group targets by file path
        let mut grouped: HashMap<String, Vec<InsertionTarget>> = HashMap::new();
        for target in targets_map.into_values() {
            grouped
                .entry(target.file_path.clone())
                .or_default()
                .push(target);
        }

        // Sort targets within each file by line number
        for targets in grouped.values_mut() {
            targets.sort_by_key(|t| t.line);
        }

        Ok((grouped, stats, skipped_hardcoded))
    }

    fn execute(
        &self,
        targets: HashMap<String, Vec<InsertionTarget>>,
        stats: InsertionStats,
        skipped_hardcoded: Vec<HardcodedIssue>,
    ) -> Result<RunResult> {
        let file_count = targets.len();
        let total_insertable: usize = targets.values().map(|v| v.len()).sum();
        let skip_count = skipped_hardcoded.len();

        // Sort file paths for deterministic output
        let mut sorted_paths: Vec<_> = targets.keys().collect();
        sorted_paths.sort();

        // Show warnings for skipped hardcoded issues first
        if !skipped_hardcoded.is_empty() {
            println!("{} (line has translation call):", "Skipped".yellow().bold());
            for issue in &skipped_hardcoded {
                self.preview_skipped_issue(&issue.location.file_path, issue);
            }
        }

        if self.apply {
            // Actually insert comments
            for file_path in &sorted_paths {
                let file_targets = targets.get(*file_path).unwrap();
                self.preview_targets(file_path, file_targets);
                self.insert_comments(file_path, file_targets)?;
            }

            if total_insertable > 0 {
                self.print_stats("Inserted", &stats, file_count, true);
            }

            if skip_count > 0 {
                println!(
                    "{} {} line(s) with mixed translation/hardcoded text.",
                    "Skipped".yellow().bold(),
                    skip_count
                );
            }

            Ok(RunResult {
                error_count: 0,
                warning_count: skip_count,
                exit_on_errors: true,
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            })
        } else {
            // Dry-run: preview changes
            for file_path in &sorted_paths {
                let file_targets = targets.get(*file_path).unwrap();
                self.preview_targets(file_path, file_targets);
            }

            if total_insertable > 0 {
                self.print_stats("Would insert", &stats, file_count, false);
                println!("Run with {} to insert these comments.", "--apply".cyan());
            }

            if skip_count > 0 {
                println!(
                    "{} {} line(s) with mixed translation/hardcoded text.",
                    "Skipped".yellow().bold(),
                    skip_count
                );
            }

            // Dry-run: report count but don't exit with error code
            Ok(RunResult {
                error_count: total_insertable,
                warning_count: skip_count,
                exit_on_errors: false, // dry-run should not fail CI
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            })
        }
    }

    /// Print statistics about inserted/would-insert comments.
    fn print_stats(&self, action: &str, stats: &InsertionStats, file_count: usize, is_apply: bool) {
        let total = stats.hardcoded + stats.untranslated;
        if is_apply {
            println!(
                "{} {} comment(s) in {} file(s):",
                action.green().bold(),
                total,
                file_count
            );
        } else {
            println!(
                "{} {} comment(s) in {} file(s):",
                action.yellow().bold(),
                total,
                file_count
            );
        }
        if stats.hardcoded > 0 {
            println!("  - hardcoded: {} comment(s)", stats.hardcoded);
        }
        if stats.untranslated > 0 {
            println!(
                "  - untranslated: {} comment(s), {} key(s)",
                stats.untranslated, stats.untranslated_keys
            );
        }
    }

    /// Preview a skipped hardcoded issue (line has translation call).
    fn preview_skipped_issue(&self, file_path: &str, issue: &HardcodedIssue) {
        let col = issue.location.col.unwrap_or(1);
        let source_line = issue.source_line.as_deref().unwrap_or("");

        // Clickable location: --> path:line:col
        println!(
            "  {} {}:{}:{}",
            "-->".blue(),
            file_path,
            issue.location.line,
            col
        );

        // Source context with line number
        println!("     {}", "|".blue());
        println!(
            " {:>3} {} {}",
            issue.location.line.to_string().blue(),
            "|".blue(),
            source_line
        );

        // Caret pointing to column
        let prefix: String = source_line.chars().take(col - 1).collect();
        let caret_padding = UnicodeWidthStr::width(prefix.as_str());
        println!(
            "     {} {:>padding$}{}",
            "|".blue(),
            "",
            "^".yellow(),
            padding = caret_padding
        );
        println!();
    }

    /// Preview insertion targets for a file.
    fn preview_targets(&self, file_path: &str, targets: &[InsertionTarget]) {
        // Read file content once for all targets in this file
        let content = fs::read_to_string(file_path).unwrap_or_default();
        let lines: Vec<&str> = content.lines().collect();

        for target in targets {
            let comment = Self::make_comment(target.in_jsx_context, &target.rules);
            let source_line = lines
                .get(target.line.saturating_sub(1))
                .copied()
                .unwrap_or("");

            // Clickable location: --> path:line:col
            println!(
                "  {} {}:{}:{}",
                "-->".blue(),
                file_path,
                target.line,
                target.col
            );

            // Source context with line number
            println!("     {}", "|".blue());
            println!(
                " {:>3} {} {}",
                target.line.to_string().blue(),
                "|".blue(),
                source_line
            );

            // Caret pointing to column
            let prefix: String = source_line
                .chars()
                .take(target.col.saturating_sub(1))
                .collect();
            let caret_padding = UnicodeWidthStr::width(prefix.as_str());
            println!(
                "     {} {:>padding$}{}",
                "|".blue(),
                "",
                "^".green(),
                padding = caret_padding
            );

            // Comment to be inserted
            let indentation: String = source_line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            println!(
                "  {} {}{}",
                "+".green().bold(),
                indentation,
                comment.green()
            );
            println!();
        }
    }

    /// Generate comment string for the given rules.
    fn make_comment(in_jsx_context: bool, rules: &HashSet<DisableRule>) -> String {
        let rules_str = DisableRule::format_rules(rules);
        if in_jsx_context {
            format!("{{/* glot-disable-next-line {} */}}", rules_str)
        } else {
            format!("// glot-disable-next-line {}", rules_str)
        }
    }

    /// Inserts `glot-disable-next-line` comments above each target line.
    ///
    /// # Behavior
    /// - Comments are inserted from bottom to top to preserve line numbers
    /// - Each comment matches the indentation of the target line
    /// - Uses `{/* */}` for JSX context, `//` for JS context
    /// - Preserves original file newline style (CRLF or LF)
    /// - Preserves trailing newline if present
    fn insert_comments(&self, file_path: &str, targets: &[InsertionTarget]) -> Result<()> {
        let content = fs::read_to_string(file_path)?;

        // Detect original newline style (CRLF vs LF)
        let newline = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let had_trailing_newline = content.ends_with(newline);

        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // Build insertions, sorted by line descending (insert from bottom up)
        let mut insertions: Vec<CommentInsertion> = targets
            .iter()
            .map(|target| {
                let comment = Self::make_comment(target.in_jsx_context, &target.rules);

                // Get indentation from the actual source line
                let source_line = lines
                    .get(target.line.saturating_sub(1))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let indentation: String = source_line
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect();

                CommentInsertion {
                    line: target.line,
                    comment,
                    indentation,
                }
            })
            .collect();

        // Sort descending by line number (insert from bottom to preserve line numbers)
        insertions.sort_by(|a, b| b.line.cmp(&a.line));

        // Insert comments
        for insertion in insertions {
            let insert_at = insertion.line.saturating_sub(1); // Convert to 0-based

            // Validate line number is within bounds
            if insert_at > lines.len() {
                // Skip this insertion if line number is out of bounds
                // This could happen if the file was modified between scanning and insertion
                if self.ctx.verbose {
                    eprintln!(
                        "Warning: Skipping comment insertion at line {} (file has {} lines)",
                        insertion.line,
                        lines.len()
                    );
                }
                continue;
            }

            let comment_line = format!("{}{}", insertion.indentation, insertion.comment);
            lines.insert(insert_at, comment_line);
        }

        // Write back with original newline style
        let mut new_content = lines.join(newline);
        if had_trailing_newline {
            new_content.push_str(newline);
        }

        fs::write(file_path, new_content)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insertion_target_struct() {
        let target = InsertionTarget {
            file_path: "src/app.tsx".to_string(),
            line: 10,
            col: 5,
            in_jsx_context: true,
            rules: [DisableRule::Hardcoded].into_iter().collect(),
        };

        assert_eq!(target.file_path, "src/app.tsx");
        assert_eq!(target.line, 10);
        assert_eq!(target.col, 5);
        assert!(target.in_jsx_context);
        assert!(target.rules.contains(&DisableRule::Hardcoded));
    }

    #[test]
    fn test_insertion_stats_default() {
        let stats = InsertionStats::default();
        assert_eq!(stats.hardcoded, 0);
        assert_eq!(stats.untranslated, 0);
        assert_eq!(stats.untranslated_keys, 0);
    }

    #[test]
    fn test_comment_insertion_struct() {
        let insertion = CommentInsertion {
            line: 5,
            comment: "// glot-disable-next-line hardcoded".to_string(),
            indentation: "    ".to_string(),
        };

        assert_eq!(insertion.line, 5);
        assert_eq!(insertion.comment, "// glot-disable-next-line hardcoded");
        assert_eq!(insertion.indentation, "    ");
    }

    #[test]
    fn test_make_comment_js_single_rule() {
        let rules: HashSet<DisableRule> = [DisableRule::Hardcoded].into_iter().collect();
        let comment = BaselineRunner::make_comment(false, &rules);
        assert_eq!(comment, "// glot-disable-next-line hardcoded");
    }

    #[test]
    fn test_make_comment_jsx_single_rule() {
        let rules: HashSet<DisableRule> = [DisableRule::Untranslated].into_iter().collect();
        let comment = BaselineRunner::make_comment(true, &rules);
        assert_eq!(comment, "{/* glot-disable-next-line untranslated */}");
    }

    #[test]
    fn test_make_comment_js_multiple_rules() {
        let rules: HashSet<DisableRule> = [DisableRule::Hardcoded, DisableRule::Untranslated]
            .into_iter()
            .collect();
        let comment = BaselineRunner::make_comment(false, &rules);
        // Rules should be sorted alphabetically
        assert_eq!(comment, "// glot-disable-next-line hardcoded untranslated");
    }

    #[test]
    fn test_make_comment_jsx_multiple_rules() {
        let rules: HashSet<DisableRule> = [DisableRule::Untranslated, DisableRule::Hardcoded]
            .into_iter()
            .collect();
        let comment = BaselineRunner::make_comment(true, &rules);
        // Rules should be sorted alphabetically
        assert_eq!(
            comment,
            "{/* glot-disable-next-line hardcoded untranslated */}"
        );
    }
}

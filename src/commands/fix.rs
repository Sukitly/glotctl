//! Fix command: automatically insert glot-message-keys comments for dynamic translation keys.

use std::collections::HashMap;
use std::fs;

use anyhow::Result;
use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use crate::{
    RunResult,
    args::FixArgs,
    checkers::extraction::{DynamicKeyReason, DynamicKeyWarning},
    commands::context::CheckContext,
};

/// Comment templates for glot-message-keys
const JS_COMMENT_PREFIX: &str = "// glot-message-keys ";
const JSX_COMMENT_PREFIX: &str = "{/* glot-message-keys ";
const JSX_COMMENT_SUFFIX: &str = " */}";

/// Represents a comment insertion operation
#[derive(Debug, Clone)]
struct CommentInsertion {
    /// 1-based line number where comment goes (above the issue line)
    line: usize,
    /// The comment text to insert
    comment: String,
    /// Whitespace prefix to match source indentation
    indentation: String,
}

/// A fixable warning with hint
#[derive(Debug, Clone)]
struct FixableWarning {
    warning: DynamicKeyWarning,
}

/// Warnings grouped by line, with merged patterns
#[derive(Debug, Clone)]
struct LineGroup {
    /// 1-based line number
    line: usize,
    /// All patterns for this line (deduplicated)
    patterns: Vec<String>,
    /// Source line content
    source_line: String,
    /// First column for display
    col: usize,
    /// Whether any warning on this line is in JSX context
    in_jsx_context: bool,
}

/// Determine if we should use JSX comment syntax.
///
/// This combines AST context (is the `t()` call in JSX children?) with
/// line content analysis (where will the comment be inserted?).
///
/// The logic:
/// - If `in_jsx_context` is false -> use `//` (not in JSX)
/// - If line starts with `<` (JSX element) -> use `{/* */}` (JSX child)
/// - If line starts with `{` (JSX expression) -> use `{/* */}` (JSX child)
/// - Otherwise -> use `//` (e.g., `return <...>` starts a JS statement)
///
/// This handles cases like:
/// - `{t(...)}` inside JSX -> needs `{/* */}` (line starts with `{`)
/// - `return <button>{t(...)}</button>` -> needs `//` (line starts with `return`)
fn should_use_jsx_comment(in_jsx_context: bool, source_line: &str) -> bool {
    if !in_jsx_context {
        return false;
    }
    let trimmed = source_line.trim_start();
    trimmed.starts_with('<') || trimmed.starts_with('{')
}

/// Runner for the fix command.
///
/// FixRunner identifies dynamic translation keys and optionally inserts
/// `glot-message-keys` comments to declare expected keys.
pub struct FixRunner {
    ctx: CheckContext,
    apply: bool,
}

impl FixRunner {
    pub fn new(args: FixArgs) -> Result<Self> {
        let ctx = CheckContext::new(&args.common)?;

        Ok(Self {
            ctx,
            apply: args.apply,
        })
    }

    pub fn run(self) -> Result<RunResult> {
        // Step 1: Ensure extractions are loaded (this uses the same logic as missing keys check)
        self.ctx.ensure_extractions()?;

        // Step 2: Collect all dynamic key warnings
        let (fixable, unfixable) = self.collect_warnings();

        if fixable.is_empty() && unfixable.is_empty() {
            println!("{}", "No dynamic keys found.".green());
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

        // Step 3: Report unfixable warnings (VariableKey without hints)
        if !unfixable.is_empty() {
            self.report_unfixable(&unfixable);
        }

        if fixable.is_empty() {
            println!(
                "\n{} No fixable dynamic keys (all are variable keys without hints).",
                "Note:".cyan().bold()
            );
            return Ok(RunResult {
                error_count: 0,
                warning_count: unfixable.len(),
                exit_on_errors: true,
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            });
        }

        // Step 4: Group by file and line, then execute
        let grouped = Self::group_by_file_and_line(&fixable);
        self.execute(grouped, unfixable.len())
    }

    /// Collects dynamic key warnings and splits into fixable/unfixable.
    ///
    /// - Fixable: TemplateWithExpr with a pattern
    /// - Unfixable: VariableKey or TemplateWithExpr without pattern
    fn collect_warnings(&self) -> (Vec<FixableWarning>, Vec<DynamicKeyWarning>) {
        let mut fixable = Vec::new();
        let mut unfixable = Vec::new();

        let extractions = match self.ctx.extractions() {
            Some(e) => e,
            None => return (fixable, unfixable),
        };

        for extraction in extractions.values() {
            for warning in &extraction.warnings {
                if warning.pattern.is_some() {
                    fixable.push(FixableWarning {
                        warning: warning.clone(),
                    });
                } else {
                    unfixable.push(warning.clone());
                }
            }
        }

        // Sort for deterministic output
        fixable.sort_by(|a, b| {
            (
                &a.warning.file_path,
                a.warning.line,
                a.warning.col,
                a.warning.pattern.as_ref(),
            )
                .cmp(&(
                    &b.warning.file_path,
                    b.warning.line,
                    b.warning.col,
                    b.warning.pattern.as_ref(),
                ))
        });
        unfixable.sort_by(|a, b| (&a.file_path, a.line).cmp(&(&b.file_path, b.line)));

        (fixable, unfixable)
    }

    fn report_unfixable(&self, unfixable: &[DynamicKeyWarning]) {
        println!(
            "{} {} dynamic key(s) (variable keys without pattern hints):\n",
            "Cannot fix".yellow().bold(),
            unfixable.len()
        );

        for warning in unfixable {
            let reason = match warning.reason {
                DynamicKeyReason::VariableKey => "variable key",
                DynamicKeyReason::TemplateWithExpr => "template (no pattern inferred)",
            };

            // Clickable location
            println!(
                "  {} {}:{}:{}",
                "-->".blue(),
                warning.file_path,
                warning.line,
                warning.col
            );

            // Source context
            println!("     {}", "|".blue());
            println!(
                " {:>3} {} {}",
                warning.line.to_string().blue(),
                "|".blue(),
                warning.source_line
            );

            // Caret
            let prefix: String = warning.source_line.chars().take(warning.col - 1).collect();
            let caret_padding = UnicodeWidthStr::width(prefix.as_str());
            println!(
                "     {} {:>padding$}{}",
                "|".blue(),
                "",
                "^".yellow(),
                padding = caret_padding
            );

            // Reason
            println!("   = {}: {}", "reason".yellow(), reason);
            println!();
        }
    }

    /// Groups warnings by file and line, merging patterns for same-line warnings.
    fn group_by_file_and_line(fixable: &[FixableWarning]) -> HashMap<String, Vec<LineGroup>> {
        let mut by_file: HashMap<String, HashMap<usize, LineGroup>> = HashMap::new();

        for fw in fixable {
            let warning = &fw.warning;
            let pattern = warning.pattern.as_ref().unwrap().clone();

            let file_entry = by_file.entry(warning.file_path.clone()).or_default();
            let line_group = file_entry.entry(warning.line).or_insert_with(|| LineGroup {
                line: warning.line,
                patterns: Vec::new(),
                source_line: warning.source_line.clone(),
                col: warning.col,
                in_jsx_context: warning.in_jsx_context,
            });

            // Add pattern if not already present (deduplicate same pattern on same line)
            if !line_group.patterns.contains(&pattern) {
                line_group.patterns.push(pattern);
            }

            // If any warning on this line is in JSX context, the group is in JSX context
            if warning.in_jsx_context {
                line_group.in_jsx_context = true;
            }
        }

        // Convert to final structure, sorted by line
        let mut result: HashMap<String, Vec<LineGroup>> = HashMap::new();
        for (file_path, line_map) in by_file {
            let mut groups: Vec<LineGroup> = line_map.into_values().collect();
            groups.sort_by_key(|g| g.line);
            result.insert(file_path, groups);
        }

        result
    }

    fn execute(
        &self,
        grouped: HashMap<String, Vec<LineGroup>>,
        unfixable_count: usize,
    ) -> Result<RunResult> {
        let file_count = grouped.len();
        let total_comments: usize = grouped.values().map(|v| v.len()).sum();

        // Sort file paths for deterministic output
        let mut sorted_paths: Vec<_> = grouped.keys().collect();
        sorted_paths.sort();

        if self.apply {
            // Actually insert comments
            for file_path in &sorted_paths {
                let line_groups = grouped.get(*file_path).unwrap();
                self.preview_changes(file_path, line_groups);
                self.insert_comments(file_path, line_groups)?;
            }

            if total_comments > 0 {
                println!(
                    "{} {} comment(s) in {} file(s).",
                    "Inserted".green().bold(),
                    total_comments,
                    file_count
                );
            }

            if unfixable_count > 0 {
                println!(
                    "{} {} dynamic key(s) could not be fixed (variable keys).",
                    "Note:".cyan().bold(),
                    unfixable_count
                );
            }

            Ok(RunResult {
                error_count: 0,
                warning_count: unfixable_count,
                exit_on_errors: true,
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            })
        } else {
            // Dry-run: preview changes
            for file_path in &sorted_paths {
                let line_groups = grouped.get(*file_path).unwrap();
                self.preview_changes(file_path, line_groups);
            }

            if total_comments > 0 {
                println!(
                    "{} {} comment(s) in {} file(s).",
                    "Would insert".yellow().bold(),
                    total_comments,
                    file_count
                );
                println!("Run with {} to insert these comments.", "--apply".cyan());
            }

            if unfixable_count > 0 {
                println!(
                    "{} {} dynamic key(s) cannot be fixed (variable keys).",
                    "Note:".cyan().bold(),
                    unfixable_count
                );
            }

            // Dry-run: report count but don't exit with error code
            Ok(RunResult {
                error_count: total_comments,
                warning_count: unfixable_count,
                exit_on_errors: false, // dry-run should not fail CI
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            })
        }
    }

    fn preview_changes(&self, file_path: &str, line_groups: &[LineGroup]) {
        for group in line_groups {
            let use_jsx = should_use_jsx_comment(group.in_jsx_context, &group.source_line);
            let comment = build_comment(&group.patterns, use_jsx);

            // Clickable location
            println!(
                "  {} {}:{}:{}",
                "-->".blue(),
                file_path,
                group.line,
                group.col
            );

            // Source context
            println!("     {}", "|".blue());
            println!(
                " {:>3} {} {}",
                group.line.to_string().blue(),
                "|".blue(),
                group.source_line
            );

            // Caret
            let prefix: String = group.source_line.chars().take(group.col - 1).collect();
            let caret_padding = UnicodeWidthStr::width(prefix.as_str());
            println!(
                "     {} {:>padding$}{}",
                "|".blue(),
                "",
                "^".green(),
                padding = caret_padding
            );

            // Comment to be inserted
            let indentation: String = group
                .source_line
                .chars()
                .take_while(|c: &char| c.is_whitespace())
                .collect();
            println!(
                "   {} {}{}",
                "+".green().bold(),
                indentation,
                comment.green()
            );
            println!();
        }
    }

    /// Inserts `glot-message-keys` comments above each dynamic key line.
    ///
    /// # Behavior
    /// - Comments are inserted from bottom to top to preserve line numbers
    /// - Each comment matches the indentation of the source line
    /// - Uses `{/* */}` for JSX context (line starts with <), `//` otherwise
    /// - Preserves original file newline style (CRLF or LF)
    /// - Preserves trailing newline if present
    /// - Multiple patterns on the same line are merged into one comment
    fn insert_comments(&self, file_path: &str, line_groups: &[LineGroup]) -> Result<()> {
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
        let mut insertions: Vec<CommentInsertion> = line_groups
            .iter()
            .map(|group| {
                let use_jsx = should_use_jsx_comment(group.in_jsx_context, &group.source_line);
                let comment = build_comment(&group.patterns, use_jsx);

                let indentation: String = group
                    .source_line
                    .chars()
                    .take_while(|c: &char| c.is_whitespace())
                    .collect();

                CommentInsertion {
                    line: group.line,
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

/// Builds the comment string with one or more patterns.
///
/// Uses the target line to determine JSX vs JS comment syntax.
fn build_comment(patterns: &[String], use_jsx: bool) -> String {
    let patterns_str = patterns
        .iter()
        .map(|p| format!("\"{}\"", p))
        .collect::<Vec<_>>()
        .join(", ");

    if use_jsx {
        format!(
            "{}{}{}",
            JSX_COMMENT_PREFIX, patterns_str, JSX_COMMENT_SUFFIX
        )
    } else {
        format!("{}{}", JS_COMMENT_PREFIX, patterns_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_comment_single_pattern_jsx() {
        let comment = build_comment(&["Common.status.*".to_string()], true);
        assert_eq!(comment, "{/* glot-message-keys \"Common.status.*\" */}");
    }

    #[test]
    fn test_build_comment_single_pattern_js() {
        let comment = build_comment(&["Common.error.*".to_string()], false);
        assert_eq!(comment, "// glot-message-keys \"Common.error.*\"");
    }

    #[test]
    fn test_build_comment_multiple_patterns_jsx() {
        let patterns = vec!["Pattern.*.x".to_string(), "Pattern.*.y".to_string()];
        let comment = build_comment(&patterns, true);
        assert_eq!(
            comment,
            "{/* glot-message-keys \"Pattern.*.x\", \"Pattern.*.y\" */}"
        );
    }

    #[test]
    fn test_build_comment_multiple_patterns_js() {
        let patterns = vec!["Pattern.*.x".to_string(), "Pattern.*.y".to_string()];
        let comment = build_comment(&patterns, false);
        assert_eq!(
            comment,
            "// glot-message-keys \"Pattern.*.x\", \"Pattern.*.y\""
        );
    }

    #[test]
    fn test_should_use_jsx_comment_jsx_element() {
        // Line starts with `<` and in JSX context -> JSX comment
        assert!(should_use_jsx_comment(true, "<span>{t(`key`)}</span>"));
        assert!(should_use_jsx_comment(true, "    <div>content</div>"));
    }

    #[test]
    fn test_should_use_jsx_comment_jsx_expression() {
        // Line starts with `{` and in JSX context -> JSX comment
        assert!(should_use_jsx_comment(true, "{t(`key`)}"));
        assert!(should_use_jsx_comment(true, "  {/* comment */}"));
        assert!(should_use_jsx_comment(true, "{cond && <span />}"));
    }

    #[test]
    fn test_should_use_jsx_comment_js_statement_with_jsx() {
        // Line starts with JS keyword but in JSX context -> JS comment
        // (because comment is inserted ABOVE the line, which is JS context)
        assert!(!should_use_jsx_comment(
            true,
            "return <span>{t(`key`)}</span>;"
        ));
        assert!(!should_use_jsx_comment(
            true,
            "const x = <div>{t(`key`)}</div>;"
        ));
    }

    #[test]
    fn test_should_use_jsx_comment_not_in_jsx_context() {
        // Not in JSX context -> always JS comment
        assert!(!should_use_jsx_comment(false, "<span>{t(`key`)}</span>"));
        assert!(!should_use_jsx_comment(false, "const x = t(`key`);"));
        assert!(!should_use_jsx_comment(false, "    console.log(t(`key`));"));
    }

    #[test]
    fn test_comment_constants() {
        assert_eq!(JS_COMMENT_PREFIX, "// glot-message-keys ");
        assert_eq!(JSX_COMMENT_PREFIX, "{/* glot-message-keys ");
        assert_eq!(JSX_COMMENT_SUFFIX, " */}");
    }
}

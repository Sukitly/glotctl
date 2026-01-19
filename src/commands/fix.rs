//! Fix command: automatically insert glot-message-keys comments for dynamic translation keys.

use std::collections::HashMap;
use std::fs;

use anyhow::Result;
use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use crate::{
    RunResult,
    args::FixArgs,
    checkers::missing_keys::{DynamicKeyReason, DynamicKeyWarning},
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

        // Step 4: Group by file and execute
        let grouped = Self::group_by_file(&fixable);
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
            (&a.warning.file_path, a.warning.line).cmp(&(&b.warning.file_path, b.warning.line))
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

    fn group_by_file(fixable: &[FixableWarning]) -> HashMap<String, Vec<FixableWarning>> {
        let mut grouped: HashMap<String, Vec<FixableWarning>> = HashMap::new();

        for fw in fixable {
            grouped
                .entry(fw.warning.file_path.clone())
                .or_default()
                .push(fw.clone());
        }

        // Sort by line within each file and deduplicate by line
        for warnings in grouped.values_mut() {
            warnings.sort_by_key(|w| w.warning.line);
            warnings.dedup_by_key(|w| w.warning.line);
        }

        grouped
    }

    fn execute(
        &self,
        grouped: HashMap<String, Vec<FixableWarning>>,
        unfixable_count: usize,
    ) -> Result<RunResult> {
        let file_count = grouped.len();
        let total_fixable: usize = grouped.values().map(|v| v.len()).sum();

        // Sort file paths for deterministic output
        let mut sorted_paths: Vec<_> = grouped.keys().collect();
        sorted_paths.sort();

        if self.apply {
            // Actually insert comments
            for file_path in &sorted_paths {
                let warnings = grouped.get(*file_path).unwrap();
                self.preview_changes(file_path, warnings);
                self.insert_comments(file_path, warnings)?;
            }

            if total_fixable > 0 {
                println!(
                    "{} {} comment(s) in {} file(s).",
                    "Inserted".green().bold(),
                    total_fixable,
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
                let warnings = grouped.get(*file_path).unwrap();
                self.preview_changes(file_path, warnings);
            }

            if total_fixable > 0 {
                println!(
                    "{} {} comment(s) in {} file(s).",
                    "Would insert".yellow().bold(),
                    total_fixable,
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
                error_count: total_fixable,
                warning_count: unfixable_count,
                exit_on_errors: false, // dry-run should not fail CI
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            })
        }
    }

    fn preview_changes(&self, file_path: &str, warnings: &[FixableWarning]) {
        for fw in warnings {
            let warning = &fw.warning;
            let pattern = warning.pattern.as_ref().unwrap();
            let comment = self.build_comment(pattern, warning.in_jsx_context);

            // Clickable location
            println!(
                "  {} {}:{}:{}",
                "-->".blue(),
                file_path,
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
                "^".green(),
                padding = caret_padding
            );

            // Comment to be inserted
            let indentation: String = warning
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

    /// Builds the comment string based on context (JSX or JS).
    fn build_comment(&self, pattern: &str, in_jsx_context: bool) -> String {
        if in_jsx_context {
            format!(
                "{}\"{}\"{}",
                JSX_COMMENT_PREFIX, pattern, JSX_COMMENT_SUFFIX
            )
        } else {
            format!("{}\"{}\"", JS_COMMENT_PREFIX, pattern)
        }
    }

    /// Inserts `glot-message-keys` comments above each dynamic key line.
    ///
    /// # Behavior
    /// - Comments are inserted from bottom to top to preserve line numbers
    /// - Each comment matches the indentation of the source line
    /// - Uses `{/* */}` for JSX context, `//` for JS context
    /// - Preserves original file newline style (CRLF or LF)
    /// - Preserves trailing newline if present
    fn insert_comments(&self, file_path: &str, warnings: &[FixableWarning]) -> Result<()> {
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
        let mut insertions: Vec<CommentInsertion> = warnings
            .iter()
            .map(|fw| {
                let warning = &fw.warning;
                let pattern = warning.pattern.as_ref().unwrap();
                let comment = self.build_comment(pattern, warning.in_jsx_context);

                let indentation: String = warning
                    .source_line
                    .chars()
                    .take_while(|c: &char| c.is_whitespace())
                    .collect();

                CommentInsertion {
                    line: warning.line,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build comment string based on context (test helper version).
    fn build_comment(pattern: &str, in_jsx_context: bool) -> String {
        if in_jsx_context {
            format!(
                "{}\"{}\"{}",
                JSX_COMMENT_PREFIX, pattern, JSX_COMMENT_SUFFIX
            )
        } else {
            format!("{}\"{}\"", JS_COMMENT_PREFIX, pattern)
        }
    }

    #[test]
    fn test_build_comment_jsx_context() {
        let comment = build_comment("Common.status.*", true);
        assert_eq!(comment, "{/* glot-message-keys \"Common.status.*\" */}");
    }

    #[test]
    fn test_build_comment_js_context() {
        let comment = build_comment("Common.error.*", false);
        assert_eq!(comment, "// glot-message-keys \"Common.error.*\"");
    }

    #[test]
    fn test_comment_constants() {
        assert_eq!(JS_COMMENT_PREFIX, "// glot-message-keys ");
        assert_eq!(JSX_COMMENT_PREFIX, "{/* glot-message-keys ");
        assert_eq!(JSX_COMMENT_SUFFIX, " */}");
    }
}

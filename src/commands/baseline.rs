use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use crate::{
    RunResult,
    args::BaselineArgs,
    checkers::hardcoded::{HardcodedChecker, HardcodedIssue},
    checkers::translation_calls::TranslationCallFinder,
    commands::context::CheckContext,
    parsers::jsx::parse_jsx_file,
    reporter::SUCCESS_MARK,
};

/// Comment to insert for baseline suppression
const JS_COMMENT: &str = "// glot-disable-next-line";
const JSX_COMMENT: &str = "{/* glot-disable-next-line */}";

/// Result of collecting hardcoded issues and translation call lines.
type IssuesCollection = (Vec<HardcodedIssue>, HashMap<String, HashSet<usize>>);

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

/// Runner for the baseline command.
///
/// BaselineRunner identifies hardcoded text and optionally inserts
/// `glot-disable-next-line` comments to suppress them.
pub struct BaselineRunner {
    ctx: CheckContext,
    apply: bool,
}

impl BaselineRunner {
    pub fn new(args: BaselineArgs) -> Result<Self> {
        let ctx = CheckContext::new(&args.common)?;

        Ok(Self {
            ctx,
            apply: args.apply,
        })
    }

    pub fn run(self) -> Result<RunResult> {
        // Step 1: Collect all hardcoded issues and translation call lines
        let (issues, translation_lines) = self.collect_issues()?;

        if issues.is_empty() {
            println!(
                "{} {}",
                SUCCESS_MARK.green(),
                "No hardcoded text found.".green()
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

        // Step 2: Group by file and deduplicate by line
        let grouped = Self::group_by_file(&issues);

        // Step 3: Execute (dry-run or apply)
        self.execute(grouped, &translation_lines)
    }

    /// Collects hardcoded issues and lines with translation calls.
    ///
    /// Returns a tuple of (issues, translation_lines_per_file).
    fn collect_issues(&self) -> Result<IssuesCollection> {
        let mut all_issues = Vec::new();
        let mut translation_lines: HashMap<String, HashSet<usize>> = HashMap::new();

        for file_path in &self.ctx.files {
            let parsed = match parse_jsx_file(Path::new(file_path)) {
                Ok(p) => p,
                Err(e) => {
                    if self.ctx.verbose {
                        eprintln!("Warning: {} - {}", file_path, e);
                    }
                    continue;
                }
            };

            // Find hardcoded issues
            let checker = HardcodedChecker::new(
                file_path,
                &self.ctx.config.checked_attributes,
                &self.ctx.ignore_texts,
                &parsed.source_map,
                &parsed.comments,
            );
            let issues = checker.check(&parsed.module);
            all_issues.extend(issues);

            // Find lines with translation calls (using AST, not string matching)
            let lines = TranslationCallFinder::new(&parsed.source_map).find(&parsed.module);
            if !lines.is_empty() {
                translation_lines.insert(file_path.clone(), lines);
            }
        }

        Ok((all_issues, translation_lines))
    }

    fn group_by_file(issues: &[HardcodedIssue]) -> HashMap<String, Vec<HardcodedIssue>> {
        let mut grouped: HashMap<String, Vec<HardcodedIssue>> = HashMap::new();

        for issue in issues {
            grouped
                .entry(issue.file_path.clone())
                .or_default()
                .push(issue.clone());
        }

        // Deduplicate: only keep one issue per line (first occurrence)
        for issues in grouped.values_mut() {
            issues.sort_by_key(|i| i.line);
            issues.dedup_by_key(|i| i.line);
        }

        grouped
    }

    fn execute(
        &self,
        grouped: HashMap<String, Vec<HardcodedIssue>>,
        translation_lines: &HashMap<String, HashSet<usize>>,
    ) -> Result<RunResult> {
        // Split issues into insertable and skipped (has translation call on same line)
        let mut insertable: HashMap<String, Vec<HardcodedIssue>> = HashMap::new();
        let mut skipped: Vec<HardcodedIssue> = Vec::new();

        for (file_path, issues) in &grouped {
            let file_translation_lines = translation_lines.get(file_path);
            for issue in issues {
                let has_translation =
                    file_translation_lines.is_some_and(|lines| lines.contains(&issue.line));
                if has_translation {
                    skipped.push(issue.clone());
                } else {
                    insertable
                        .entry(file_path.clone())
                        .or_default()
                        .push(issue.clone());
                }
            }
        }

        // Sort skipped for deterministic output
        skipped.sort_by(|a, b| (&a.file_path, a.line).cmp(&(&b.file_path, b.line)));

        let file_count = insertable.len();
        let total_insertable: usize = insertable.values().map(|v| v.len()).sum();
        let skip_count = skipped.len();

        // Sort file paths for deterministic output
        let mut sorted_paths: Vec<_> = insertable.keys().collect();
        sorted_paths.sort();

        // Show warnings for skipped issues first
        if !skipped.is_empty() {
            println!("{} (line has translation call):", "Skipped".yellow().bold());
            for issue in &skipped {
                self.preview_issue(&issue.file_path, issue);
            }
        }

        if self.apply {
            // Actually insert comments
            for file_path in &sorted_paths {
                let issues = insertable.get(*file_path).unwrap();
                self.preview_changes(file_path, issues);
                self.insert_comments(file_path, issues)?;
            }

            if total_insertable > 0 {
                println!(
                    "{} {} comment(s) in {} file(s).",
                    "Inserted".green().bold(),
                    total_insertable,
                    file_count
                );
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
                let issues = insertable.get(*file_path).unwrap();
                self.preview_changes(file_path, issues);
            }

            if total_insertable > 0 {
                println!(
                    "{} {} comment(s) in {} file(s).",
                    "Would insert".yellow().bold(),
                    total_insertable,
                    file_count
                );
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

    fn preview_issue(&self, file_path: &str, issue: &HardcodedIssue) {
        // Clickable location: --> path:line:col
        println!(
            "  {} {}:{}:{}",
            "-->".blue(),
            file_path,
            issue.line,
            issue.col
        );

        // Source context with line number
        println!("     {}", "|".blue());
        println!(
            " {:>3} {} {}",
            issue.line.to_string().blue(),
            "|".blue(),
            issue.source_line
        );

        // Caret pointing to column
        let prefix: String = issue.source_line.chars().take(issue.col - 1).collect();
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

    fn preview_changes(&self, file_path: &str, issues: &[HardcodedIssue]) {
        for issue in issues {
            let comment = if issue.in_jsx_context {
                JSX_COMMENT
            } else {
                JS_COMMENT
            };

            // Clickable location: --> path:line:col
            println!(
                "  {} {}:{}:{}",
                "-->".blue(),
                file_path,
                issue.line,
                issue.col
            );

            // Source context with line number
            println!("     {}", "|".blue());
            println!(
                " {:>3} {} {}",
                issue.line.to_string().blue(),
                "|".blue(),
                issue.source_line
            );

            // Caret pointing to column
            let prefix: String = issue.source_line.chars().take(issue.col - 1).collect();
            let caret_padding = UnicodeWidthStr::width(prefix.as_str());
            println!(
                "     {} {:>padding$}{}",
                "|".blue(),
                "",
                "^".green(),
                padding = caret_padding
            );

            // Comment to be inserted
            let indentation: String = issue
                .source_line
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

    /// Inserts `glot-disable-next-line` comments above each issue line.
    ///
    /// # Behavior
    /// - Comments are inserted from bottom to top to preserve line numbers
    /// - Each comment matches the indentation of the issue line
    /// - Uses `{/* */}` for JSX context, `//` for JS context
    /// - Preserves original file newline style (CRLF or LF)
    /// - Preserves trailing newline if present
    fn insert_comments(&self, file_path: &str, issues: &[HardcodedIssue]) -> Result<()> {
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
        let mut insertions: Vec<CommentInsertion> = issues
            .iter()
            .map(|issue| {
                let comment = if issue.in_jsx_context {
                    JSX_COMMENT.to_string()
                } else {
                    JS_COMMENT.to_string()
                };

                let indentation: String = issue
                    .source_line
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect();

                CommentInsertion {
                    line: issue.line,
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

    fn create_test_issue(file_path: &str, line: usize, in_jsx_context: bool) -> HardcodedIssue {
        HardcodedIssue {
            file_path: file_path.to_string(),
            line,
            col: 1,
            text: "test text".to_string(),
            source_line: "    const x = <div>test text</div>".to_string(),
            in_jsx_context,
        }
    }

    #[test]
    fn test_group_by_file_single_file() {
        let issues = vec![
            create_test_issue("src/app.tsx", 10, true),
            create_test_issue("src/app.tsx", 20, true),
        ];

        let grouped = BaselineRunner::group_by_file(&issues);

        assert_eq!(grouped.len(), 1);
        assert!(grouped.contains_key("src/app.tsx"));
        assert_eq!(grouped.get("src/app.tsx").unwrap().len(), 2);
    }

    #[test]
    fn test_group_by_file_multiple_files() {
        let issues = vec![
            create_test_issue("src/app.tsx", 10, true),
            create_test_issue("src/utils.ts", 5, false),
            create_test_issue("src/app.tsx", 20, true),
        ];

        let grouped = BaselineRunner::group_by_file(&issues);

        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("src/app.tsx").unwrap().len(), 2);
        assert_eq!(grouped.get("src/utils.ts").unwrap().len(), 1);
    }

    #[test]
    fn test_group_by_file_deduplicates_same_line() {
        let issues = vec![
            create_test_issue("src/app.tsx", 10, true),
            create_test_issue("src/app.tsx", 10, true), // Same line
            create_test_issue("src/app.tsx", 20, true),
        ];

        let grouped = BaselineRunner::group_by_file(&issues);

        // Should only have 2 issues (line 10 deduplicated)
        assert_eq!(grouped.get("src/app.tsx").unwrap().len(), 2);
    }

    #[test]
    fn test_group_by_file_sorted_by_line() {
        let issues = vec![
            create_test_issue("src/app.tsx", 30, true),
            create_test_issue("src/app.tsx", 10, true),
            create_test_issue("src/app.tsx", 20, true),
        ];

        let grouped = BaselineRunner::group_by_file(&issues);
        let file_issues = grouped.get("src/app.tsx").unwrap();

        assert_eq!(file_issues[0].line, 10);
        assert_eq!(file_issues[1].line, 20);
        assert_eq!(file_issues[2].line, 30);
    }

    #[test]
    fn test_comment_insertion_struct() {
        let insertion = CommentInsertion {
            line: 5,
            comment: "// glot-disable-next-line".to_string(),
            indentation: "    ".to_string(),
        };

        assert_eq!(insertion.line, 5);
        assert_eq!(insertion.comment, "// glot-disable-next-line");
        assert_eq!(insertion.indentation, "    ");
    }

    #[test]
    fn test_js_vs_jsx_comment_constants() {
        assert_eq!(JS_COMMENT, "// glot-disable-next-line");
        assert_eq!(JSX_COMMENT, "{/* glot-disable-next-line */}");
    }
}

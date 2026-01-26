//! Report formatting and printing utilities.
//!
//! This module provides functions to display issues in cargo-style format.
//! Separate from core logic to allow glot to be used as a library.

use std::io::{self, Write};

use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use super::{
    actions::{Action, DeleteKey, InsertDisableComment, InsertMessageKeys},
    commands::{
        BaselineSummary, CleanSummary, CommandResult, CommandSummary, FixSummary, InitSummary,
    },
};
use crate::config::CONFIG_FILE_NAME;
use crate::core::ResolvedKeyUsage;
use crate::issues::{Issue, Report, ReportLocation, Severity, UnresolvedKeyIssue};

/// Success mark for consistent output formatting.
pub const SUCCESS_MARK: &str = "\u{2713}"; // ✓

/// Failure mark for consistent output formatting.
pub const FAILURE_MARK: &str = "\u{2718}"; // ✘

/// Maximum number of usages to display per issue.
const MAX_USAGES_DISPLAY: usize = 3;

/// Print issues in cargo-style format to stdout.
///
/// This is the main entry point for reporting. Issues are sorted and
/// displayed with severity, location, source context, and details.
///
/// # Example
///
/// ```ignore
/// use glot::report::report;
/// use glot::issues::Issue;
///
/// let issues: Vec<Issue> = checker.check(&data);
/// report(&issues);
/// ```
pub fn report(issues: &[Issue]) {
    report_to(issues, &mut io::stdout().lock());
}

/// Print issues to a custom writer.
///
/// Useful for testing or redirecting output.
pub fn report_to<W: Write>(issues: &[Issue], writer: &mut W) {
    if issues.is_empty() {
        return;
    }

    let mut sorted = issues.to_vec();
    sorted.sort_by(compare_issues);

    // Calculate max line number width for alignment
    let max_line_width = calculate_max_line_width(&sorted);

    for issue in &sorted {
        print_issue(issue, writer, max_line_width);
    }

    print_summary(&sorted, writer);
}

/// Print a success message when no issues are found.
pub fn print_success(source_files: usize, locale_files: usize) {
    print_success_to(source_files, locale_files, &mut io::stdout().lock());
}

/// Print a success message to a custom writer.
pub fn print_success_to<W: Write>(source_files: usize, locale_files: usize, writer: &mut W) {
    let msg = if locale_files == 0 {
        format!(
            "{} {}",
            SUCCESS_MARK.green(),
            format!(
                "Checked {} source {} - no issues found",
                source_files,
                if source_files == 1 { "file" } else { "files" }
            )
            .green()
        )
    } else {
        format!(
            "{} {}",
            SUCCESS_MARK.green(),
            format!(
                "Checked {} source {}, {} locale {} - no issues found",
                source_files,
                if source_files == 1 { "file" } else { "files" },
                locale_files,
                if locale_files == 1 { "file" } else { "files" }
            )
            .green()
        )
    };
    let _ = writeln!(writer, "{}", msg);
}

/// Print a warning about files that could not be parsed.
pub fn print_parse_warning(count: usize, verbose: bool) {
    print_parse_warning_to(count, verbose, &mut io::stderr().lock());
}

/// Print a parse warning to a custom writer.
pub fn print_parse_warning_to<W: Write>(count: usize, verbose: bool, writer: &mut W) {
    if count > 0 && !verbose {
        let _ = writeln!(
            writer,
            "{} {} file(s) could not be parsed (use {} for details)",
            "warning:".bold().yellow(),
            count,
            "-v".cyan()
        );
    }
}

// ============================================================
// Internal Functions
// ============================================================

fn print_issue<W: Write>(issue: &Issue, writer: &mut W, max_line_width: usize) {
    let loc = issue.location();
    let (file_path, line, col, source_line) = extract_location_info(&loc);

    // Print severity and message (cargo-style)
    let severity = issue.report_severity();
    let severity_str = match severity {
        Severity::Error => "error".bold().red(),
        Severity::Warning => "warning".bold().yellow(),
    };

    let _ = writeln!(
        writer,
        "{}: \"{}\"  {}",
        severity_str,
        issue.message(),
        issue.report_rule().to_string().dimmed().cyan()
    );

    // Print clickable location: --> path:line:col
    let _ = writeln!(writer, "  {} {}:{}:{}", "-->".blue(), file_path, line, col);

    // Print source context if available
    if let Some(source_line) = source_line {
        let caret_char = match severity {
            Severity::Error => "^".red(),
            Severity::Warning => "^".yellow(),
        };

        let _ = writeln!(
            writer,
            "{:>width$} {}",
            "",
            "|".blue(),
            width = max_line_width
        );
        let _ = writeln!(
            writer,
            "{:>width$} {} {}",
            line.to_string().blue(),
            "|".blue(),
            source_line,
            width = max_line_width
        );

        // Caret pointing to the column (col is 1-based)
        let prefix = if col > 1 {
            source_line.chars().take(col - 1).collect::<String>()
        } else {
            String::new()
        };
        let caret_padding = UnicodeWidthStr::width(prefix.as_str());
        let _ = writeln!(
            writer,
            "{:>width$} {} {:>padding$}{}",
            "",
            "|".blue(),
            "",
            caret_char,
            width = max_line_width,
            padding = caret_padding
        );
    }

    // Print details if present (cargo-style note)
    if let Some(details) = issue.details() {
        let _ = writeln!(
            writer,
            "{:>width$} {} {} {}",
            "",
            "=".blue(),
            "note:".bold(),
            details,
            width = max_line_width
        );
    }

    // Print hint if present
    if let Some(hint) = issue.hint() {
        let _ = writeln!(
            writer,
            "{:>width$} {} {} {}",
            "",
            "=".blue(),
            "hint:".bold().cyan(),
            hint,
            width = max_line_width
        );
    }

    // Print usages if present
    let usages = issue.usages();
    if !usages.is_empty() {
        print_usages(usages, writer, max_line_width);
    } else if matches!(
        issue,
        Issue::ReplicaLag(_) | Issue::Untranslated(_) | Issue::TypeMismatch(_)
    ) {
        // These issues should show usages, but none were found
        let _ = writeln!(
            writer,
            "{:>width$} {} {} {}",
            "",
            "=".blue(),
            "used:".bold(),
            "(no usages found)".dimmed(),
            width = max_line_width
        );
    }

    let _ = writeln!(writer); // Empty line between issues
}

fn print_usages<W: Write>(usages: &[ResolvedKeyUsage], writer: &mut W, max_line_width: usize) {
    let total = usages.len();
    let display_count = total.min(MAX_USAGES_DISPLAY);

    for (i, usage) in usages.iter().take(display_count).enumerate() {
        let is_last = i == display_count - 1;
        let remaining = total.saturating_sub(display_count);
        let suffix = if is_last && remaining > 0 {
            format!(" (and {} more)", remaining)
        } else {
            String::new()
        };

        let _ = writeln!(
            writer,
            "{:>width$} {} {} {}:{}:{}{}",
            "",
            "=".blue(),
            "used:".bold(),
            usage.context.file_path(),
            usage.context.line(),
            usage.context.col(),
            suffix,
            width = max_line_width
        );
    }
}

fn print_summary<W: Write>(issues: &[Issue], writer: &mut W) {
    let total_errors = issues
        .iter()
        .filter(|i| i.report_severity() == Severity::Error)
        .count();
    let total_warnings = issues
        .iter()
        .filter(|i| i.report_severity() == Severity::Warning)
        .count();
    let total_problems = total_errors + total_warnings;

    if total_problems > 0 {
        let _ = writeln!(
            writer,
            "\n{} {} problems ({} {}, {} {})",
            FAILURE_MARK.red(),
            total_problems,
            total_errors,
            if total_errors == 1 { "error" } else { "errors" }.red(),
            total_warnings,
            if total_warnings == 1 {
                "warning"
            } else {
                "warnings"
            }
            .yellow()
        );
    }
}

fn extract_location_info<'a>(
    loc: &'a ReportLocation<'a>,
) -> (&'a str, usize, usize, Option<&'a str>) {
    match loc {
        ReportLocation::Source(ctx) => (
            ctx.file_path(),
            ctx.line(),
            ctx.col(),
            Some(&ctx.source_line),
        ),
        ReportLocation::Message(ctx) => (ctx.file_path(), ctx.line(), ctx.col(), None),
        ReportLocation::File { path } => (path, 0, 0, None),
    }
}

fn calculate_max_line_width(issues: &[Issue]) -> usize {
    issues
        .iter()
        .filter_map(|i| {
            let loc = i.location();
            match loc {
                ReportLocation::Source(ctx) => Some(ctx.line()),
                ReportLocation::Message(ctx) => Some(ctx.line()),
                ReportLocation::File { .. } => None,
            }
        })
        .max()
        .map(|n| n.to_string().len())
        .unwrap_or(1)
}

fn compare_issues(a: &Issue, b: &Issue) -> std::cmp::Ordering {
    let a_loc = a.location();
    let b_loc = b.location();
    let (a_path, a_line, a_col, _) = extract_location_info(&a_loc);
    let (b_path, b_line, b_col, _) = extract_location_info(&b_loc);

    a_path
        .cmp(b_path)
        .then_with(|| a_line.cmp(&b_line))
        .then_with(|| a_col.cmp(&b_col))
}

pub fn print(result: &CommandResult, verbose: bool) {
    print_command_output(result);

    if result.issues.is_empty() {
        print_success(result.source_files_checked, result.locale_files_checked);
    }

    print_parse_warning(result.parse_error_count, verbose);
}

fn print_command_output(result: &CommandResult) {
    match &result.summary {
        CommandSummary::Check => {
            report(&result.issues);
        }
        CommandSummary::Baseline(summary) => {
            print_baseline(summary);
        }
        CommandSummary::Fix(summary) => {
            print_fix(summary);
        }
        CommandSummary::Clean(summary) => {
            print_clean(summary);
        }
        CommandSummary::Init(summary) => {
            print_init(summary);
        }
    }
}

fn print_baseline(summary: &BaselineSummary) {
    if !summary.is_apply {
        if !summary.hardcoded_issues.is_empty() {
            InsertDisableComment::preview(&summary.hardcoded_issues);
        }
        if !summary.untranslated_issues.is_empty() {
            InsertDisableComment::preview(&summary.untranslated_issues);
        }
    }

    let total = summary.hardcoded_count + summary.untranslated_usage_count;
    if total > 0 {
        if summary.is_apply {
            println!(
                "{} {} comment(s) in {} file(s) (processed {} issue(s)):",
                "Inserted".green().bold(),
                summary.applied_total_count,
                summary.file_count,
                total
            );
            if summary.hardcoded_count > 0 {
                println!(
                    "  - hardcoded: {} comment(s) (from {} issue(s))",
                    summary.applied_hardcoded_count, summary.hardcoded_count
                );
            }
            if summary.untranslated_usage_count > 0 {
                println!(
                    "  - untranslated: {} comment(s), {} key(s) (from {} usage(s))",
                    summary.applied_untranslated_count,
                    summary.untranslated_key_count,
                    summary.untranslated_usage_count
                );
            }
        } else {
            println!(
                "{} {} comment(s) in {} file(s):",
                "Would insert".yellow().bold(),
                total,
                summary.file_count
            );
            if summary.hardcoded_count > 0 {
                println!("  - hardcoded: {} comment(s)", summary.hardcoded_count);
            }
            if summary.untranslated_usage_count > 0 {
                println!(
                    "  - untranslated: {} comment(s), {} key(s)",
                    summary.untranslated_usage_count, summary.untranslated_key_count
                );
            }
            println!("Run with {} to insert these comments.", "--apply".cyan());
        }
    }
}

fn print_fix(summary: &FixSummary) {
    let unfixable_issues: Vec<&UnresolvedKeyIssue> = summary
        .unresolved_issues
        .iter()
        .filter(|issue| issue.pattern.is_none())
        .collect();
    let has_fixable = summary.processed_count > 0;
    let has_unfixable = !unfixable_issues.is_empty();

    if has_unfixable {
        print_unfixable_keys(&unfixable_issues);
    }

    if !summary.is_apply && !summary.unresolved_issues.is_empty() {
        InsertMessageKeys::preview(&summary.unresolved_issues);
    }

    if summary.unresolved_count > 0 {
        if summary.is_apply {
            if has_fixable {
                println!(
                    "{} {} comment(s) in {} file(s) (processed {} issue(s)).",
                    "Inserted".green().bold(),
                    summary.applied_count,
                    summary.file_count,
                    summary.processed_count
                );
                if summary.skipped_count > 0 {
                    println!(
                        "  - skipped: {} issue(s) without pattern",
                        summary.skipped_count
                    );
                }
            }
        } else if has_fixable {
            println!(
                "{} {} comment(s) in {} file(s).",
                "Would insert".yellow().bold(),
                summary.processed_count,
                summary.file_count
            );
            println!("Run with {} to insert these comments.", "--apply".cyan());
        }

        if has_unfixable && !summary.is_apply {
            if has_fixable {
                println!(
                    "Note: {} dynamic key(s) cannot be fixed (variable keys).",
                    summary.skipped_count
                );
            } else {
                println!();
                println!("Note: No fixable dynamic keys (all are variable keys without hints).");
            }
        }
    }
}

fn print_unfixable_keys(issues: &[&UnresolvedKeyIssue]) {
    println!(
        "{} Cannot fix {} unresolved key(s) (variable keys without pattern hints):",
        FAILURE_MARK.red(),
        issues.len()
    );
    println!();

    for issue in issues {
        let ctx = &issue.context;
        let line = ctx.line();
        let col = ctx.col();
        let source_line = &ctx.source_line;

        println!("  {} {}:{}:{}", "-->".blue(), ctx.file_path(), line, col);
        println!("     {}", "|".blue());
        println!(
            " {:>3} {} {}",
            line.to_string().blue(),
            "|".blue(),
            source_line
        );

        let prefix: String = source_line.chars().take(col.saturating_sub(1)).collect();
        let caret_padding = UnicodeWidthStr::width(prefix.as_str());
        println!(
            "     {} {:>padding$}{}",
            "|".blue(),
            "",
            "^".red(),
            padding = caret_padding
        );
        println!("   {} reason: {}", "=".blue(), issue.reason);
        println!();
    }
}

fn print_clean(summary: &CleanSummary) {
    if !summary.is_apply {
        if !summary.unused_issues.is_empty() {
            DeleteKey::preview(&summary.unused_issues);
        }
        if !summary.orphan_issues.is_empty() {
            DeleteKey::preview(&summary.orphan_issues);
        }
    }

    let total = summary.unused_count + summary.orphan_count;
    if total > 0 {
        if summary.is_apply {
            println!(
                "{} {} key(s) in {} file(s) (processed {} key(s)).",
                "Deleted".green().bold(),
                summary.applied_total_count,
                summary.file_count,
                total
            );
            if summary.unused_count > 0 {
                println!(
                    "  - unused: {} key(s) (from {} issue(s))",
                    summary.applied_unused_count, summary.unused_count
                );
            }
            if summary.orphan_count > 0 {
                println!(
                    "  - orphan: {} key(s) (from {} issue(s))",
                    summary.applied_orphan_count, summary.orphan_count
                );
            }
        } else {
            println!(
                "{} {} key(s) in {} file(s).",
                "Would delete".yellow().bold(),
                total,
                summary.file_count
            );
            println!("Run with {} to delete these keys.", "--apply".cyan());
        }
    }
}

fn print_init(summary: &InitSummary) {
    if summary.created {
        println!(
            "{} {}",
            SUCCESS_MARK.green(),
            format!("Created {}", CONFIG_FILE_NAME).green()
        );
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::core::FullKey;
    use crate::core::{
        CommentStyle, LocaleTypeMismatch, MessageContext, MessageLocation, SourceContext,
        SourceLocation, ValueType,
    };
    use crate::issues::{
        HardcodedTextIssue, IssueUnresolvedKeyReason, MissingKeyIssue, OrphanKeyIssue,
        ParseErrorIssue, ReplicaLagIssue, TypeMismatchIssue, UnresolvedKeyIssue, UntranslatedIssue,
        UnusedKeyIssue,
    };

    fn strip_ansi(s: &str) -> String {
        // Simple ANSI escape code stripper for testing
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip until 'm'
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    #[test]
    fn test_report_empty() {
        let mut output = Vec::new();
        report_to(&[], &mut output);
        assert!(output.is_empty());
    }

    #[test]
    fn test_report_hardcoded_issue() {
        let loc = SourceLocation::new("./src/app.tsx", 10, 5);
        let ctx = SourceContext::new(loc, "const x = \"Hello\";", CommentStyle::Js);
        let issue = Issue::HardcodedText(HardcodedTextIssue {
            context: ctx,
            text: "Hello".to_string(),
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("error:"));
        assert!(stripped.contains("\"Hello\""));
        assert!(stripped.contains("hardcoded"));
        assert!(stripped.contains("./src/app.tsx:10:5"));
        assert!(stripped.contains("const x = \"Hello\";"));
    }

    #[test]
    fn test_report_missing_key_with_schema() {
        let loc = SourceLocation::new("./src/form.tsx", 20, 5);
        let ctx = SourceContext::new(loc, "formSchema(t)", CommentStyle::Js);
        let issue = Issue::MissingKey(MissingKeyIssue {
            context: ctx,
            key: "Form.email".to_string(),
            from_schema: Some((
                "formSchema".to_string(),
                "./src/schemas/form.ts".to_string(),
            )),
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("error:"));
        assert!(stripped.contains("\"Form.email\""));
        assert!(stripped.contains("note:"));
        assert!(stripped.contains("from formSchema"));
    }

    #[test]
    fn test_report_unused_key() {
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Common.unused", "Unused Value");
        let issue = Issue::UnusedKey(UnusedKeyIssue { context: ctx });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("warning:"));
        assert!(stripped.contains("\"Common.unused\""));
        assert!(stripped.contains("unused-key"));
        assert!(stripped.contains("./messages/en.json:5:3"));
        assert!(stripped.contains("(\"Unused Value\")"));
    }

    #[test]
    fn test_report_with_usages() {
        let msg_loc = MessageLocation::new("./messages/en.json", 5, 3);
        let msg_ctx = MessageContext::new(msg_loc, "Common.submit", "Submit");

        let usage_loc = SourceLocation::new("./src/Button.tsx", 25, 10);
        let usage_ctx = SourceContext::new(usage_loc, "{t('Common.submit')}", CommentStyle::Jsx);
        let usage = ResolvedKeyUsage {
            key: FullKey::new("Common.submit"),
            context: usage_ctx,
            suppressed_rules: HashSet::new(),
            from_schema: None,
        };

        let issue = Issue::ReplicaLag(ReplicaLagIssue {
            context: msg_ctx,
            primary_locale: "en".to_string(),
            missing_in: vec!["zh".to_string(), "ja".to_string()],
            usages: vec![usage],
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("error:"));
        assert!(stripped.contains("missing in: zh, ja"));
        assert!(stripped.contains("used:"));
        assert!(stripped.contains("./src/Button.tsx:25:10"));
    }

    #[test]
    fn test_report_summary() {
        let loc1 = SourceLocation::new("./src/a.tsx", 10, 5);
        let ctx1 = SourceContext::new(loc1, "\"Error\"", CommentStyle::Js);
        let issue1 = Issue::HardcodedText(HardcodedTextIssue {
            context: ctx1,
            text: "Error".to_string(),
        });

        let loc2 = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx2 = MessageContext::new(loc2, "Common.unused", "Unused");
        let issue2 = Issue::UnusedKey(UnusedKeyIssue { context: ctx2 });

        let mut output = Vec::new();
        report_to(&[issue1, issue2], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("2 problems"));
        assert!(stripped.contains("1 error"));
        assert!(stripped.contains("1 warning"));
    }

    #[test]
    fn test_print_success() {
        let mut output = Vec::new();
        print_success_to(10, 3, &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("10 source files"));
        assert!(stripped.contains("3 locale files"));
        assert!(stripped.contains("no issues found"));
    }

    #[test]
    fn test_print_success_no_locales() {
        let mut output = Vec::new();
        print_success_to(5, 0, &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("5 source files"));
        assert!(!stripped.contains("locale"));
    }

    #[test]
    fn test_report_unresolved_key_with_hint() {
        let loc = SourceLocation::new("./src/app.tsx", 15, 8);
        let ctx = SourceContext::new(loc, "t(`status.${code}`)", CommentStyle::Jsx);
        let issue = Issue::UnresolvedKey(UnresolvedKeyIssue {
            context: ctx,
            reason: IssueUnresolvedKeyReason::TemplateWithExpr,
            hint: Some("Use glot-message-keys annotation".to_string()),
            pattern: Some("status.*".to_string()),
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("warning:"));
        assert!(stripped.contains("template with expression"));
        assert!(stripped.contains("unresolved-key"));
        assert!(stripped.contains("hint:"));
        assert!(stripped.contains("glot-message-keys"));
    }

    #[test]
    fn test_report_orphan_key() {
        let loc = MessageLocation::new("./messages/zh.json", 10, 3);
        let ctx = MessageContext::new(loc, "Common.orphan", "孤儿值");
        let issue = Issue::OrphanKey(OrphanKeyIssue {
            context: ctx,
            locale: "zh".to_string(),
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("warning:"));
        assert!(stripped.contains("\"Common.orphan\""));
        assert!(stripped.contains("orphan-key"));
        assert!(stripped.contains("in zh"));
        assert!(stripped.contains("(\"孤儿值\")"));
    }

    #[test]
    fn test_report_untranslated() {
        let msg_loc = MessageLocation::new("./messages/en.json", 5, 3);
        let msg_ctx = MessageContext::new(msg_loc, "Common.ok", "OK");

        let issue = Issue::Untranslated(UntranslatedIssue {
            context: msg_ctx,
            primary_locale: "en".to_string(),
            identical_in: vec!["zh".to_string(), "ja".to_string()],
            usages: vec![],
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("warning:"));
        assert!(stripped.contains("\"Common.ok\""));
        assert!(stripped.contains("untranslated"));
        assert!(stripped.contains("identical in: zh, ja"));
        assert!(stripped.contains("(no usages found)"));
    }

    #[test]
    fn test_report_type_mismatch() {
        let msg_loc = MessageLocation::new("./messages/en.json", 8, 3);
        let msg_ctx = MessageContext::new(msg_loc, "Page.items", "[\"a\", \"b\"]");

        let mismatch_loc = MessageLocation::new("./messages/zh.json", 8, 3);
        let mismatch = LocaleTypeMismatch::new("zh", ValueType::String, mismatch_loc);

        let issue = Issue::TypeMismatch(TypeMismatchIssue {
            context: msg_ctx,
            expected_type: ValueType::StringArray,
            primary_locale: "en".to_string(),
            mismatched_in: vec![mismatch],
            usages: vec![],
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("error:"));
        assert!(stripped.contains("\"Page.items\""));
        assert!(stripped.contains("type-mismatch"));
        assert!(stripped.contains("expected array"));
        assert!(stripped.contains("zh (string)"));
    }

    #[test]
    fn test_report_unresolved_key_unknown_namespace() {
        let loc = SourceLocation::new("./src/form.tsx", 30, 5);
        let ctx = SourceContext::new(loc, "schema(t)", CommentStyle::Js);
        let issue = Issue::UnresolvedKey(UnresolvedKeyIssue {
            context: ctx,
            reason: IssueUnresolvedKeyReason::UnknownNamespace {
                schema_name: "formSchema".to_string(),
            },
            hint: None,
            pattern: None,
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("warning:"));
        assert!(stripped.contains("unknown namespace for schema 'formSchema'"));
        assert!(stripped.contains("unresolved-key"));
    }

    #[test]
    fn test_report_parse_error() {
        let issue = Issue::ParseError(ParseErrorIssue {
            file_path: "./src/broken.tsx".to_string(),
            error: "Unexpected token at line 5".to_string(),
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        assert!(stripped.contains("error:"));
        assert!(stripped.contains("Unexpected token at line 5"));
        assert!(stripped.contains("parse-error"));
        assert!(stripped.contains("./src/broken.tsx"));
    }

    #[test]
    fn test_report_usages_truncation() {
        let msg_loc = MessageLocation::new("./messages/en.json", 5, 3);
        let msg_ctx = MessageContext::new(msg_loc, "Common.key", "Value");

        // Create 5 usages (more than MAX_USAGES_DISPLAY = 3)
        let usages: Vec<ResolvedKeyUsage> = (1..=5)
            .map(|i| {
                let loc = SourceLocation::new(format!("./src/file{}.tsx", i), i * 10, 5);
                let ctx = SourceContext::new(loc, "t('Common.key')", CommentStyle::Js);
                ResolvedKeyUsage {
                    key: FullKey::new("Common.key"),
                    context: ctx,
                    suppressed_rules: HashSet::new(),
                    from_schema: None,
                }
            })
            .collect();

        let issue = Issue::ReplicaLag(ReplicaLagIssue {
            context: msg_ctx,
            primary_locale: "en".to_string(),
            missing_in: vec!["zh".to_string()],
            usages,
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();
        let stripped = strip_ansi(&output_str);

        // Should show 3 usages and "(and 2 more)"
        assert!(stripped.contains("./src/file1.tsx:10:5"));
        assert!(stripped.contains("./src/file2.tsx:20:5"));
        assert!(stripped.contains("./src/file3.tsx:30:5"));
        assert!(stripped.contains("(and 2 more)"));
        assert!(!stripped.contains("./src/file4.tsx"));
        assert!(!stripped.contains("./src/file5.tsx"));
    }

    #[test]
    fn test_report_sorting_by_file_and_line() {
        // Create issues in random order
        let loc1 = SourceLocation::new("./src/b.tsx", 20, 5);
        let ctx1 = SourceContext::new(loc1, "\"B20\"", CommentStyle::Js);
        let issue1 = Issue::HardcodedText(HardcodedTextIssue {
            context: ctx1,
            text: "B20".to_string(),
        });

        let loc2 = SourceLocation::new("./src/a.tsx", 10, 5);
        let ctx2 = SourceContext::new(loc2, "\"A10\"", CommentStyle::Js);
        let issue2 = Issue::HardcodedText(HardcodedTextIssue {
            context: ctx2,
            text: "A10".to_string(),
        });

        let loc3 = SourceLocation::new("./src/a.tsx", 5, 5);
        let ctx3 = SourceContext::new(loc3, "\"A5\"", CommentStyle::Js);
        let issue3 = Issue::HardcodedText(HardcodedTextIssue {
            context: ctx3,
            text: "A5".to_string(),
        });

        let mut output = Vec::new();
        report_to(&[issue1, issue2, issue3], &mut output);
        let output_str = String::from_utf8(output).unwrap();

        // Should be sorted: a.tsx:5, a.tsx:10, b.tsx:20
        let a5_pos = output_str.find("\"A5\"").unwrap();
        let a10_pos = output_str.find("\"A10\"").unwrap();
        let b20_pos = output_str.find("\"B20\"").unwrap();

        assert!(a5_pos < a10_pos, "a.tsx:5 should come before a.tsx:10");
        assert!(a10_pos < b20_pos, "a.tsx:10 should come before b.tsx:20");
    }

    #[test]
    fn test_report_unicode_source_line() {
        // Test that caret aligns correctly with CJK characters
        let loc = SourceLocation::new("./src/app.tsx", 10, 8);
        // "你好" takes 4 display width (2 per CJK char), then "World" starts at col 8
        let ctx = SourceContext::new(loc, "const x = \"你好World\";", CommentStyle::Js);
        let issue = Issue::HardcodedText(HardcodedTextIssue {
            context: ctx,
            text: "World".to_string(),
        });

        let mut output = Vec::new();
        report_to(&[issue], &mut output);
        let output_str = String::from_utf8(output).unwrap();

        // Just verify it doesn't panic and contains expected content
        assert!(output_str.contains("你好World"));
        assert!(output_str.contains("^"));
    }
}

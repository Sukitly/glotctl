//! Report formatting and printing utilities.
//!
//! This module is separate from the core library logic to allow glot
//! to be used as a library without printing side effects.

use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use crate::issue::{Issue, Severity};

/// Print issues in a cargo-style format.
///
/// Issues are sorted and displayed with:
/// - Severity and message
/// - Clickable file location (path:line:col)
/// - Source code context with caret indicator
/// - Notes and hints
/// - Summary of total errors/warnings
pub fn print_report(issues: &[Issue]) {
    let mut sorted = issues.to_vec();
    sorted.sort();

    // All issues now have file_path, so file_issues contains all issues
    let file_issues: Vec<_> = sorted.iter().collect();

    // Calculate max line number width for alignment
    let max_line_width = file_issues
        .iter()
        .filter_map(|i| i.line)
        .max()
        .map(|n| n.to_string().len())
        .unwrap_or(1);

    for issue in &file_issues {
        let line = issue.line.unwrap_or(0);
        let col = issue.col.unwrap_or(0);
        let path = issue.file_path.as_deref().unwrap_or("");

        // Print severity and message (cargo-style)
        let severity_str = match issue.severity {
            Severity::Error => "error".bold().red(),
            Severity::Warning => "warning".bold().yellow(),
        };

        println!(
            "{}: \"{}\"  {}",
            severity_str,
            issue.message,
            issue.rule.to_string().dimmed().cyan()
        );

        // Print clickable location: --> path:line:col
        println!("  {} {}:{}:{}", "-->".blue(), path, line, col);

        // Print source context if available
        if let Some(source_line) = &issue.source_line {
            let caret_char = match issue.severity {
                Severity::Error => "^".red(),
                Severity::Warning => "^".yellow(),
            };

            println!("{:>width$} {}", "", "|".blue(), width = max_line_width);
            println!(
                "{:>width$} {} {}",
                line.to_string().blue(),
                "|".blue(),
                source_line,
                width = max_line_width
            );
            // Caret pointing to the column (col is 1-based)
            // Use unicode display width for correct positioning with CJK chars and emoji
            let prefix = if col > 1 {
                source_line.chars().take(col - 1).collect::<String>()
            } else {
                String::new()
            };
            let caret_padding = UnicodeWidthStr::width(prefix.as_str());
            println!(
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
        if let Some(details) = &issue.details {
            println!(
                "{:>width$} {} {} {}",
                "",
                "=".blue(),
                "note:".bold(),
                details,
                width = max_line_width
            );
        }

        // Print hint if present
        if let Some(hint) = &issue.hint {
            println!(
                "{:>width$} {} {} {}",
                "",
                "=".blue(),
                "hint:".bold().cyan(),
                hint,
                width = max_line_width
            );
        }

        println!(); // Empty line between issues
    }

    // Summary
    let total_errors = sorted
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    let total_warnings = sorted
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let total_problems = total_errors + total_warnings;

    if total_problems > 0 {
        println!(
            "\n{} {} problems ({} {}, {} {})",
            "✖".red(),
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

/// Print a warning about files that could not be parsed.
///
/// This is shown at the end of a check run when files were skipped.
pub fn print_parse_warning(parse_error_count: usize, verbose: bool) {
    if parse_error_count > 0 && !verbose {
        eprintln!(
            "Warning: {} file(s) could not be parsed (use -v for details)",
            parse_error_count
        );
    }
}

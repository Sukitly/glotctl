//! Fix command - Insert glot-message-keys comments for unresolved keys.
//!
//! This command automatically fixes unresolved key warnings by inserting
//! `glot-message-keys "pattern"` comments that declare the expected dynamic keys.
//!
//! Only unresolved key issues with a valid pattern suggestion can be fixed.
//! Issues without a pattern (e.g., completely dynamic keys) are skipped and reported.
//!
//! Use `--apply` to actually insert comments (default is dry-run mode).

use std::collections::HashSet;

use anyhow::Result;
use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use super::super::{
    actions::{Action, ActionStats, InsertMessageKeys},
    args::FixCommand,
    exit_status::ExitStatus,
    report::{self, FAILURE_MARK},
};
use crate::{
    core::CheckContext,
    issues::{Rule, UnresolvedKeyIssue},
    rules::unresolved::check_unresolved_keys_issues,
};

pub fn fix(cmd: FixCommand, verbose: bool) -> Result<ExitStatus> {
    let args = &cmd.args;
    let ctx = CheckContext::new(&args.common)?;
    let apply = args.apply;

    let unresolved_issues: Vec<UnresolvedKeyIssue> = check_unresolved_keys_issues(&ctx);
    let unresolved_count = unresolved_issues.len();

    let (processed_count, applied_count, skipped_count, file_count) = if apply {
        let stats = if unresolved_issues.is_empty() {
            ActionStats::default()
        } else {
            InsertMessageKeys::run(&unresolved_issues)?
        };
        (
            stats.processed,
            stats.changes_applied,
            stats.skipped,
            stats.files_modified,
        )
    } else {
        let mut files: HashSet<&str> = HashSet::new();
        for issue in &unresolved_issues {
            files.insert(issue.context.file_path());
        }
        let fixable = unresolved_issues
            .iter()
            .filter(|issue| issue.pattern.is_some())
            .count();
        let skipped = unresolved_issues
            .iter()
            .filter(|issue| issue.pattern.is_none())
            .count();
        (fixable, 0, skipped, files.len())
    };

    // Print output
    let unfixable_issues: Vec<&UnresolvedKeyIssue> = unresolved_issues
        .iter()
        .filter(|issue| issue.pattern.is_none())
        .collect();
    let has_fixable = processed_count > 0;
    let has_unfixable = !unfixable_issues.is_empty();

    if unresolved_count == 0 {
        report::print_no_issue(ctx.files.len(), ctx.messages().all_messages.len());
    } else {
        // Print unfixable keys first
        if has_unfixable {
            print_unfixable_keys(&unfixable_issues);
        }

        // Show preview in dry-run mode
        if !apply && !unresolved_issues.is_empty() {
            InsertMessageKeys::preview(&unresolved_issues);
        }

        if apply {
            if has_fixable {
                println!(
                    "{} {} comment(s) in {} file(s) (processed {} issue(s)).",
                    "Inserted".green().bold(),
                    applied_count,
                    file_count,
                    processed_count
                );
                if skipped_count > 0 {
                    println!("  - skipped: {} issue(s) without pattern", skipped_count);
                }
            }
        } else if has_fixable {
            println!(
                "{} {} comment(s) in {} file(s).",
                "Would insert".yellow().bold(),
                processed_count,
                file_count
            );
            println!("Run with {} to insert these comments.", "--apply".cyan());
        }

        if has_unfixable && !apply {
            if has_fixable {
                println!(
                    "Note: {} dynamic key(s) cannot be fixed (variable keys).",
                    skipped_count
                );
            } else {
                println!();
                println!("Note: No fixable dynamic keys (all are variable keys without hints).");
            }
        }
    }

    let parse_error_count = ctx.parsed_files_errors().len();
    report::print_parse_error(parse_error_count, verbose);

    // Determine exit status
    // In dry-run mode, finding issues is considered "Failure" (exit 1)
    // to signal that there's work to be done
    if parse_error_count > 0 {
        Ok(ExitStatus::Error)
    } else if unresolved_count > 0 && !apply {
        Ok(ExitStatus::Failure)
    } else {
        Ok(ExitStatus::Success)
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

        println!(
            "  {} {}:{}:{}  {}",
            "-->".blue(),
            ctx.file_path(),
            line,
            col,
            format!("[{}]", Rule::UnresolvedKey).dimmed().cyan()
        );
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

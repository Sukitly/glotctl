//! Clean command - Remove unused or orphan keys from message files.
//!
//! This command deletes keys from JSON message files:
//! - `unused`: Keys that exist in message files but are never used in code
//! - `orphan`: Keys that exist in non-primary locale files but not in primary locale
//!
//! The command performs safety checks before deletion:
//! - Blocks if any message files failed to parse
//! - Blocks if any unresolved key warnings exist (dynamic keys can't be tracked)
//!
//! Use `--apply` to actually delete keys (default is dry-run mode).

use std::collections::HashSet;

use anyhow::Result;
use colored::Colorize;

use super::super::{
    actions::{Action, ActionStats, DeleteKey},
    args::{CleanCommand, CleanRule},
    exit_status::ExitStatus,
    report::{self, FAILURE_MARK},
};
use crate::{
    core::CheckContext,
    issues::{Issue, OrphanKeyIssue, UnresolvedKeyIssue, UnusedKeyIssue},
    rules::{
        orphan::check_orphan_keys_issues, unresolved::check_unresolved_keys_issues,
        unused::check_unused_keys_issues,
    },
};

impl CleanRule {
    pub fn all() -> HashSet<Self> {
        [Self::Unused, Self::Orphan].into_iter().collect()
    }
}

pub fn clean(cmd: CleanCommand, verbose: bool) -> Result<ExitStatus> {
    let args = &cmd.args;
    let ctx = CheckContext::new(&args.common)?;
    let apply = args.apply;

    // Check for message parse errors - block clean if any message files failed to parse
    let message_parse_errors = ctx.message_parse_errors();
    if !message_parse_errors.is_empty() {
        eprintln!(
            "Error: {} Cannot clean, {} file(s) could not be parsed.",
            FAILURE_MARK.red(),
            message_parse_errors.len()
        );
        eprintln!("Parse errors mean some files could not be analyzed.");
        eprintln!("Run `glot check` to see details and fix them.");

        let issues: Vec<Issue> = message_parse_errors
            .iter()
            .map(|i| Issue::ParseError(i.clone()))
            .collect();
        report::report_to_stderr(&issues);

        return Ok(ExitStatus::Error);
    }

    // Check for unresolved keys - block clean if any keys cannot be statically resolved
    let unresolved_issues: Vec<UnresolvedKeyIssue> = check_unresolved_keys_issues(&ctx);
    if !unresolved_issues.is_empty() {
        eprintln!(
            "Error: {} Cannot clean, {} unresolved key warning(s) found.",
            FAILURE_MARK.red(),
            unresolved_issues.len()
        );
        eprintln!("Unresolved keys prevent tracking all key usage.");
        eprintln!("Run `glot check` to see details, then fix or suppress them.");

        let issues: Vec<Issue> = unresolved_issues
            .into_iter()
            .map(Issue::UnresolvedKey)
            .collect();
        report::report_to_stderr(&issues);

        return Ok(ExitStatus::Error);
    }

    let rules = if args.rules.is_empty() {
        CleanRule::all()
    } else {
        args.rules.clone().into_iter().collect()
    };

    let mut unused_issues: Vec<UnusedKeyIssue> = Vec::new();
    let mut orphan_issues: Vec<OrphanKeyIssue> = Vec::new();

    for rule in rules {
        match rule {
            CleanRule::Unused => {
                let issues = check_unused_keys_issues(&ctx);
                unused_issues.extend(issues);
            }
            CleanRule::Orphan => {
                let issues = check_orphan_keys_issues(&ctx);
                orphan_issues.extend(issues);
            }
        }
    }

    let unused_count = unused_issues.len();
    let orphan_count = orphan_issues.len();
    let total = unused_count + orphan_count;

    let (file_count, applied_unused_count, applied_orphan_count, applied_total_count) = if apply {
        let mut stats = ActionStats::default();
        let mut unused_stats = ActionStats::default();
        let mut orphan_stats = ActionStats::default();

        if !unused_issues.is_empty() {
            unused_stats = DeleteKey::run(&unused_issues)?;
            stats += unused_stats.clone();
        }
        if !orphan_issues.is_empty() {
            orphan_stats = DeleteKey::run(&orphan_issues)?;
            stats += orphan_stats.clone();
        }

        (
            stats.files_modified,
            unused_stats.changes_applied,
            orphan_stats.changes_applied,
            unused_stats.changes_applied + orphan_stats.changes_applied,
        )
    } else {
        let mut files: HashSet<&str> = HashSet::new();
        for issue in &unused_issues {
            files.insert(issue.context.file_path());
        }
        for issue in &orphan_issues {
            files.insert(issue.context.file_path());
        }
        (files.len(), 0, 0, 0)
    };

    // Print output
    if total == 0 {
        report::print_no_issue(ctx.files.len(), ctx.messages().all_messages.len());
    } else {
        // Show preview in dry-run mode
        if !apply {
            if !unused_issues.is_empty() {
                DeleteKey::preview(&unused_issues);
            }
            if !orphan_issues.is_empty() {
                DeleteKey::preview(&orphan_issues);
            }
        }

        if apply {
            println!(
                "{} {} key(s) in {} file(s) (processed {} key(s)).",
                "Deleted".green().bold(),
                applied_total_count,
                file_count,
                total
            );
            if unused_count > 0 {
                println!(
                    "  - unused: {} key(s) (from {} issue(s))",
                    applied_unused_count, unused_count
                );
            }
            if orphan_count > 0 {
                println!(
                    "  - orphan: {} key(s) (from {} issue(s))",
                    applied_orphan_count, orphan_count
                );
            }
        } else {
            println!(
                "{} {} unused key(s) and {} orphan key(s) from {} file(s).",
                "Would delete".yellow().bold(),
                unused_count,
                orphan_count,
                file_count
            );
            println!("Run with {} to delete these keys.", "--apply".cyan());
        }
    }

    let parse_error_count = ctx.parsed_files_errors().len();
    report::print_parse_error(parse_error_count, verbose);

    // Determine exit status
    // In dry-run mode, finding issues to clean is considered "Failure" (exit 1)
    // to signal that there's work to be done
    if parse_error_count > 0 {
        Ok(ExitStatus::Error)
    } else if total > 0 && !apply {
        Ok(ExitStatus::Failure)
    } else {
        Ok(ExitStatus::Success)
    }
}

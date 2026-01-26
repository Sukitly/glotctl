use std::collections::HashSet;

use anyhow::{Ok, Result};
use colored::Colorize;

use crate::{
    actions::{Action, ActionStats, InsertMessageKeys},
    args::FixCommand,
    commands::RunResult,
    commands::{context::CheckContext, helper::finish},
    issues::{Issue, UnresolvedKeyIssue},
    rules::unresolved::check_unresolved_keys_issues,
};

pub fn fix(cmd: FixCommand) -> Result<RunResult> {
    let args = &cmd.args;
    let ctx = CheckContext::new(&args.common)?;
    let apply = args.apply;

    let unresolved_issues: Vec<UnresolvedKeyIssue> = check_unresolved_keys_issues(&ctx);
    let unresolved_count = unresolved_issues.len();

    if apply {
        let stats = if unresolved_issues.is_empty() {
            ActionStats::default()
        } else {
            InsertMessageKeys::run(&unresolved_issues)?
        };

        print_stats(
            "Inserted",
            unresolved_count,
            stats.processed,
            stats.skipped,
            stats.files_modified,
            true,
        );
    } else {
        if !unresolved_issues.is_empty() {
            InsertMessageKeys::preview(&unresolved_issues);
        }

        let mut files: HashSet<&str> = HashSet::new();
        for issue in &unresolved_issues {
            files.insert(issue.context.file_path());
        }

        print_stats(
            "Would insert",
            unresolved_count,
            unresolved_issues
                .iter()
                .filter(|issue| issue.pattern.is_some())
                .count(),
            unresolved_issues
                .iter()
                .filter(|issue| issue.pattern.is_none())
                .count(),
            files.len(),
            false,
        );
    }

    let parse_errors = ctx.parsed_files_errors();

    let mut all_issues: Vec<Issue> = Vec::new();
    all_issues.extend(unresolved_issues.into_iter().map(Issue::UnresolvedKey));
    all_issues.extend(parse_errors.iter().map(|i| Issue::ParseError(i.clone())));

    Ok(finish(
        all_issues,
        ctx.files.len(),
        ctx.messages().all_messages.len(),
    ))
}

fn print_stats(
    action: &str,
    unresolved: usize,
    inserted: usize,
    skipped: usize,
    file_count: usize,
    is_apply: bool,
) {
    if unresolved > 0 {
        if is_apply {
            println!(
                "{} {} comment(s) in {} file(s):",
                action.green().bold(),
                unresolved,
                file_count
            );
            if inserted > 0 {
                println!("  - inserted: {} comment(s)", inserted);
            }
            if skipped > 0 {
                println!("  - skipped: {} issue(s) without pattern", skipped);
            }
        } else {
            println!(
                "{} {} comment(s) in {} file(s):",
                action.yellow().bold(),
                unresolved,
                file_count
            );
            println!("Run with {} to insert these comments.", "--apply".cyan());
        }
    }
}

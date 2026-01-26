use std::collections::HashSet;

use anyhow::{Ok, Result};
use colored::Colorize;

use crate::{
    actions::{Action, ActionStats, DeleteKey},
    args::{CleanCommand, CleanRule},
    commands::{context::CheckContext, helper::finish},
    rules::{orphan::check_orphan_keys_issues, unused::check_unused_keys_issues},
    types::{
        issue::{Issue, OrphanKeyIssue, UnusedKeyIssue},
        run_result::RunResult,
    },
};

impl CleanRule {
    pub fn all() -> HashSet<Self> {
        [Self::Unused, Self::Orphan].into_iter().collect()
    }
}

pub fn clean(cmd: CleanCommand) -> Result<RunResult> {
    let args = &cmd.args;
    let ctx = CheckContext::new(&args.common)?;
    let apply = args.apply;

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

    if apply {
        let mut stats = ActionStats::default();
        if !unused_issues.is_empty() {
            stats += DeleteKey::run(&unused_issues)?;
        }
        if !orphan_issues.is_empty() {
            stats += DeleteKey::run(&orphan_issues)?;
        }

        print_stats(
            "Deleted",
            unused_count,
            orphan_count,
            stats.files_modified,
            true,
        );
    } else {
        if !unused_issues.is_empty() {
            DeleteKey::preview(&unused_issues);
        }
        if !orphan_issues.is_empty() {
            DeleteKey::preview(&orphan_issues);
        }

        let mut files: HashSet<&str> = HashSet::new();
        for issue in &unused_issues {
            files.insert(issue.context.file_path());
        }
        for issue in &orphan_issues {
            files.insert(issue.context.file_path());
        }

        print_stats(
            "Would delete",
            unused_count,
            orphan_count,
            files.len(),
            false,
        );
        println!("Run with {} to delete these keys.", "--apply".cyan());
    }

    let parse_errors = ctx.parsed_files_errors();

    let mut all_issues: Vec<Issue> = Vec::new();
    all_issues.extend(unused_issues.into_iter().map(Issue::UnusedKey));
    all_issues.extend(orphan_issues.into_iter().map(Issue::OrphanKey));
    all_issues.extend(parse_errors.iter().map(|i| Issue::ParseError(i.clone())));

    Ok(finish(
        all_issues,
        ctx.files.len(),
        ctx.messages().all_messages.len(),
    ))
}

fn print_stats(action: &str, unused: usize, orphan: usize, file_count: usize, is_apply: bool) {
    let total = unused + orphan;
    if is_apply {
        println!(
            "{} {} key(s) in {} file(s):",
            action.green().bold(),
            total,
            file_count
        );
    } else {
        println!(
            "{} {} key(s) in {} file(s):",
            action.yellow().bold(),
            total,
            file_count
        );
    }

    if unused > 0 {
        println!("  - unused: {} key(s)", unused);
    }
    if orphan > 0 {
        println!("  - orphan: {} key(s)", orphan);
    }
}

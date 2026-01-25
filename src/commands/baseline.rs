use std::collections::HashSet;

use crate::{
    actions::{Action, ActionStats, InsertDisableComment, Operation},
    args::BaselineCommand,
    commands::{context::CheckContext, helper::finish},
    extraction::collect::SuppressibleRule,
    rules::{hardcoded::check_hardcoded_issues, untranslated::check_untranslated_issues},
    types::{
        issue::{HardcodedIssue, Issue, UntranslatedIssue},
        run_result::RunResult,
    },
};
use anyhow::{Ok, Result};
use colored::Colorize;

impl SuppressibleRule {
    pub fn default() -> Vec<SuppressibleRule> {
        vec![SuppressibleRule::Hardcoded, SuppressibleRule::Untranslated]
    }
}

pub fn baseline(cmd: BaselineCommand) -> Result<RunResult> {
    let args = &cmd.args;
    let rules = &cmd.args.rules;
    let ctx = CheckContext::new(&args.common)?;
    let apply = args.apply;

    let rules = if rules.is_empty() {
        SuppressibleRule::default()
    } else {
        rules.clone()
    };

    let mut hardcoded_issues: Vec<HardcodedIssue> = Vec::new();
    let mut untranslated_issues: Vec<UntranslatedIssue> = Vec::new();
    let mut operations: Vec<Operation> = Vec::new();

    for rule in rules {
        match rule {
            SuppressibleRule::Hardcoded => {
                let issues = check_hardcoded_issues(&ctx);
                let ops = InsertDisableComment::to_operations(&issues);
                operations.extend(ops);
                hardcoded_issues.extend(issues);
            }
            SuppressibleRule::Untranslated => {
                let issues = check_untranslated_issues(&ctx);
                let ops = InsertDisableComment::to_operations(&issues);
                operations.extend(ops);
                untranslated_issues.extend(issues);
            }
        }
    }

    let hardcoded_count = hardcoded_issues.len();
    let untranslated_usage_count: usize = untranslated_issues.iter().map(|u| u.usages.len()).sum();
    let untranslated_key_count = untranslated_issues.len();

    if apply {
        // Execute
        let mut stats = ActionStats::default();
        if !hardcoded_issues.is_empty() {
            stats += InsertDisableComment::run(&hardcoded_issues)?;
        }
        if !untranslated_issues.is_empty() {
            stats += InsertDisableComment::run(&untranslated_issues)?;
        }

        print_stats(
            "Inserted",
            hardcoded_count,
            untranslated_usage_count,
            untranslated_key_count,
            stats.files_modified,
            true,
        );
    } else {
        // Dry-run: preview changes
        if !hardcoded_issues.is_empty() {
            InsertDisableComment::preview(&hardcoded_issues);
        }
        if !untranslated_issues.is_empty() {
            InsertDisableComment::preview(&untranslated_issues);
        }

        // Count unique files
        let mut files: HashSet<&str> = HashSet::new();
        for issue in &hardcoded_issues {
            files.insert(issue.context.file_path());
        }
        for issue in &untranslated_issues {
            for usage in &issue.usages {
                files.insert(usage.file_path());
            }
        }

        print_stats(
            "Would insert",
            hardcoded_count,
            untranslated_usage_count,
            untranslated_key_count,
            files.len(),
            false,
        );
        println!("Run with {} to insert these comments.", "--apply".cyan());
    }

    let parse_errors = ctx.parsed_files_errors();

    let mut all_issues: Vec<Issue> = Vec::new();
    all_issues.extend(hardcoded_issues.into_iter().map(Issue::Hardcoded));
    all_issues.extend(untranslated_issues.into_iter().map(Issue::Untranslated));
    all_issues.extend(parse_errors.iter().map(|i| Issue::ParseError(i.clone())));

    Ok(finish(
        all_issues,
        ctx.files.len(),
        ctx.messages().all_messages.len(),
    ))
}

fn print_stats(
    action: &str,
    hardcoded: usize,
    untranslated: usize,
    untranslated_keys: usize,
    file_count: usize,
    is_apply: bool,
) {
    let total = hardcoded + untranslated;
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
    if hardcoded > 0 {
        println!("  - hardcoded: {} comment(s)", hardcoded);
    }
    if untranslated > 0 {
        println!(
            "  - untranslated: {} comment(s), {} key(s)",
            untranslated, untranslated_keys
        );
    }
}

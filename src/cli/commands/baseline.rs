use std::collections::HashSet;

use anyhow::Result;
use colored::Colorize;

use super::super::{
    actions::{execute_operations, Action, ActionStats, InsertDisableComment},
    args::BaselineCommand,
    exit_status::ExitStatus,
    report,
};
use crate::{
    core::{collect::SuppressibleRule, CheckContext},
    issues::{HardcodedTextIssue, UntranslatedIssue},
    rules::{hardcoded::check_hardcoded_text_issues, untranslated::check_untranslated_issues},
};

pub fn baseline(cmd: BaselineCommand, verbose: bool) -> Result<ExitStatus> {
    let args = &cmd.args;
    let rules = &cmd.args.rules;
    let ctx = CheckContext::new(&args.common.path, args.common.verbose)?;
    let apply = args.apply;

    let rules = if rules.is_empty() {
        SuppressibleRule::all()
    } else {
        rules.clone().into_iter().collect()
    };

    let mut hardcoded_issues: Vec<HardcodedTextIssue> = Vec::new();
    let mut untranslated_issues: Vec<UntranslatedIssue> = Vec::new();
    for rule in rules {
        match rule {
            SuppressibleRule::Hardcoded => {
                let issues = check_hardcoded_text_issues(&ctx);
                hardcoded_issues.extend(issues);
            }
            SuppressibleRule::Untranslated => {
                let issues = check_untranslated_issues(&ctx);
                untranslated_issues.extend(issues);
            }
        }
    }

    let hardcoded_count = hardcoded_issues.len();
    let untranslated_usage_count: usize = untranslated_issues.iter().map(|u| u.usages.len()).sum();
    let untranslated_key_count = untranslated_issues.len();
    let total = hardcoded_count + untranslated_usage_count;

    let (file_count, applied_hardcoded_count, applied_untranslated_count, applied_total_count) =
        if apply {
            let mut ops = Vec::new();
            if !hardcoded_issues.is_empty() {
                ops.extend(InsertDisableComment::to_operations(&hardcoded_issues));
            }
            if !untranslated_issues.is_empty() {
                ops.extend(InsertDisableComment::to_operations(&untranslated_issues));
            }

            let stats = if ops.is_empty() {
                ActionStats::default()
            } else {
                execute_operations(&ops)?
            };

            let applied_hardcoded_count = unique_hardcoded_lines(&hardcoded_issues);
            let applied_untranslated_count = unique_untranslated_lines(&untranslated_issues);

            (
                stats.files_modified,
                applied_hardcoded_count,
                applied_untranslated_count,
                stats.changes_applied,
            )
        } else {
            let mut files: HashSet<&str> = HashSet::new();
            for issue in &hardcoded_issues {
                files.insert(issue.context.file_path());
            }
            for issue in &untranslated_issues {
                for usage in &issue.usages {
                    files.insert(usage.context.file_path());
                }
            }
            (files.len(), 0, 0, 0)
        };

    // Print output
    if total == 0 {
        report::print_no_issue(ctx.files.len(), ctx.messages().all_messages.len());
    } else {
        // Show preview in dry-run mode
        if !apply {
            if !hardcoded_issues.is_empty() {
                InsertDisableComment::preview(&hardcoded_issues);
            }
            if !untranslated_issues.is_empty() {
                InsertDisableComment::preview(&untranslated_issues);
            }
        }

        if apply {
            println!(
                "{} {} comment(s) in {} file(s) (processed {} issue(s)):",
                "Inserted".green().bold(),
                applied_total_count,
                file_count,
                total
            );
            if hardcoded_count > 0 {
                println!(
                    "  - hardcoded: {} comment(s) (from {} issue(s))",
                    applied_hardcoded_count, hardcoded_count
                );
            }
            if untranslated_usage_count > 0 {
                println!(
                    "  - untranslated: {} comment(s), {} key(s) (from {} usage(s))",
                    applied_untranslated_count, untranslated_key_count, untranslated_usage_count
                );
            }
        } else {
            println!(
                "{} {} comment(s) in {} file(s):",
                "Would insert".yellow().bold(),
                total,
                file_count
            );
            if hardcoded_count > 0 {
                println!("  - hardcoded: {} comment(s)", hardcoded_count);
            }
            if untranslated_usage_count > 0 {
                println!(
                    "  - untranslated: {} comment(s), {} key(s)",
                    untranslated_usage_count, untranslated_key_count
                );
            }
            println!("Run with {} to insert these comments.", "--apply".cyan());
        }
    }

    let parse_error_count = ctx.parsed_files_errors().len();
    report::print_parse_error(parse_error_count, verbose);

    // Determine exit status
    // In dry-run mode, finding issues is considered "Failure" (exit 1)
    // to signal that there's work to be done
    if parse_error_count > 0 {
        Ok(ExitStatus::Error)
    } else if total > 0 && !apply {
        Ok(ExitStatus::Failure)
    } else {
        Ok(ExitStatus::Success)
    }
}

fn unique_hardcoded_lines(issues: &[HardcodedTextIssue]) -> usize {
    let mut unique = HashSet::new();
    for issue in issues {
        unique.insert((issue.context.file_path().to_string(), issue.context.line()));
    }
    unique.len()
}

fn unique_untranslated_lines(issues: &[UntranslatedIssue]) -> usize {
    let mut unique = HashSet::new();
    for issue in issues {
        for usage in &issue.usages {
            unique.insert((usage.context.file_path().to_string(), usage.context.line()));
        }
    }
    unique.len()
}

//! Baseline command - Suppress existing issues with disable comments.
//!
//! This command inserts `glot-disable-next-line` comments to suppress existing issues,
//! allowing you to establish a baseline and fix issues incrementally.
//!
//! Supported rules:
//! - `hardcoded`: Suppress hardcoded text issues
//! - `untranslated`: Suppress untranslated value issues
//!
//! Use `--apply` to actually insert comments (default is dry-run mode).

use std::collections::HashSet;

use anyhow::Result;
use colored::Colorize;

use super::super::{
    actions::{Action, ActionStats, InsertDisableComment, execute_operations},
    args::BaselineCommand,
    exit_status::ExitStatus,
    report::{self, FAILURE_MARK},
};
use crate::{
    core::{CheckContext, collect::SuppressibleRule},
    issues::{HardcodedTextIssue, UntranslatedIssue},
    rules::{hardcoded::check_hardcoded_text_issues, untranslated::check_untranslated_issues},
};

pub fn baseline(cmd: BaselineCommand, verbose: bool) -> Result<ExitStatus> {
    let args = &cmd.args;
    let rules = &cmd.args.rules;
    let ctx = CheckContext::new(&args.common)?;
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

    let suppressible_untranslated_issues: Vec<UntranslatedIssue> = untranslated_issues
        .iter()
        .filter(|issue| !issue.usages.is_empty())
        .cloned()
        .collect();
    let unsuppressible_untranslated_issues: Vec<&UntranslatedIssue> = untranslated_issues
        .iter()
        .filter(|issue| issue.usages.is_empty())
        .collect();

    let hardcoded_count = hardcoded_issues.len();
    let untranslated_usage_count: usize = suppressible_untranslated_issues
        .iter()
        .map(|u| u.usages.len())
        .sum();
    let untranslated_key_count = suppressible_untranslated_issues.len();
    let unsuppressible_untranslated_count = unsuppressible_untranslated_issues.len();
    let comment_total = hardcoded_count + untranslated_usage_count;
    let has_issues = comment_total > 0 || unsuppressible_untranslated_count > 0;

    let (file_count, applied_hardcoded_count, applied_untranslated_count, applied_total_count) =
        if apply {
            let mut ops = Vec::new();
            if !hardcoded_issues.is_empty() {
                ops.extend(InsertDisableComment::to_operations(&hardcoded_issues));
            }
            if !suppressible_untranslated_issues.is_empty() {
                ops.extend(InsertDisableComment::to_operations(
                    &suppressible_untranslated_issues,
                ));
            }

            let stats = if ops.is_empty() {
                ActionStats::default()
            } else {
                execute_operations(&ops)?
            };

            let applied_hardcoded_count = unique_hardcoded_lines(&hardcoded_issues);
            let applied_untranslated_count =
                unique_untranslated_lines(&suppressible_untranslated_issues);

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
            for issue in &suppressible_untranslated_issues {
                for usage in &issue.usages {
                    files.insert(usage.context.file_path());
                }
            }
            (files.len(), 0, 0, 0)
        };

    // Print output
    if !has_issues {
        report::print_no_issue(ctx.files.len(), ctx.messages().all_messages.len());
    } else {
        // Show preview in dry-run mode
        if !apply {
            if !hardcoded_issues.is_empty() {
                InsertDisableComment::preview(&hardcoded_issues);
            }
            if !suppressible_untranslated_issues.is_empty() {
                InsertDisableComment::preview(&suppressible_untranslated_issues);
            }
        }

        if apply {
            if comment_total > 0 {
                println!(
                    "{} {} comment(s) in {} file(s) (processed {} issue(s)):",
                    "Inserted".green().bold(),
                    applied_total_count,
                    file_count,
                    comment_total
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
                        applied_untranslated_count,
                        untranslated_key_count,
                        untranslated_usage_count
                    );
                }
            }
        } else if comment_total > 0 {
            println!(
                "{} {} comment(s) in {} file(s):",
                "Would insert".yellow().bold(),
                comment_total,
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

        if unsuppressible_untranslated_count > 0 {
            print_untranslated_without_usages(&unsuppressible_untranslated_issues);
        }
    }

    let parse_error_count = ctx.parsed_files_errors().len();
    report::print_parse_error(parse_error_count, verbose);

    // Determine exit status
    // In dry-run mode, finding issues is considered "Failure" (exit 1)
    // to signal that there's work to be done
    if parse_error_count > 0 {
        Ok(ExitStatus::Error)
    } else if (comment_total > 0 && !apply) || unsuppressible_untranslated_count > 0 {
        Ok(ExitStatus::Failure)
    } else {
        Ok(ExitStatus::Success)
    }
}

fn print_untranslated_without_usages(issues: &[&UntranslatedIssue]) {
    eprintln!(
        "Error: {} {} untranslated key issue(s) cannot be suppressed with source comments because no usages were found.",
        FAILURE_MARK.red(),
        issues.len()
    );
    for issue in issues {
        eprintln!(
            "  - {} at {}:{}",
            issue.context.key,
            issue.context.file_path(),
            issue.context.line()
        );
    }
    eprintln!(
        "Translate these values, remove unused keys, or configure `severities.untranslated` for a project-wide policy."
    );
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

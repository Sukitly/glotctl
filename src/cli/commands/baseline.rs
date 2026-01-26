use std::collections::HashSet;

use super::super::{
    actions::{Action, ActionStats, InsertDisableComment, execute_operations},
    args::BaselineCommand,
};
use super::{BaselineSummary, CommandResult, CommandSummary, helper::finish};
use crate::{
    core::{CheckContext, collect::SuppressibleRule},
    issues::{HardcodedTextIssue, Issue, UntranslatedIssue},
    rules::{hardcoded::check_hardcoded_text_issues, untranslated::check_untranslated_issues},
};
use anyhow::{Ok, Result};

pub fn baseline(cmd: BaselineCommand) -> Result<CommandResult> {
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

    let hardcoded_issues_summary = hardcoded_issues.clone();
    let untranslated_issues_summary = untranslated_issues.clone();
    let parse_errors = ctx.parsed_files_errors();

    let mut all_issues: Vec<Issue> = Vec::new();
    all_issues.extend(hardcoded_issues.into_iter().map(Issue::HardcodedText));
    all_issues.extend(untranslated_issues.into_iter().map(Issue::Untranslated));
    all_issues.extend(parse_errors.iter().map(|i| Issue::ParseError(i.clone())));

    Ok(finish(
        CommandSummary::Baseline(BaselineSummary {
            hardcoded_count,
            untranslated_usage_count,
            untranslated_key_count,
            applied_hardcoded_count,
            applied_untranslated_count,
            applied_total_count,
            file_count,
            is_apply: apply,
            hardcoded_issues: hardcoded_issues_summary,
            untranslated_issues: untranslated_issues_summary,
        }),
        all_issues,
        ctx.files.len(),
        ctx.messages().all_messages.len(),
        false,
    ))
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

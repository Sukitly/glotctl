use std::collections::HashSet;

use super::super::{
    actions::{Action, ActionStats, InsertDisableComment},
    args::BaselineCommand,
};
use super::{BaselineSummary, CommandKind, CommandResult, CommandSummary, helper::finish};
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

    let file_count = if apply {
        let mut stats = ActionStats::default();
        if !hardcoded_issues.is_empty() {
            stats += InsertDisableComment::run(&hardcoded_issues)?;
        }
        if !untranslated_issues.is_empty() {
            stats += InsertDisableComment::run(&untranslated_issues)?;
        }
        stats.files_modified
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
        files.len()
    };

    let hardcoded_issues_summary = hardcoded_issues.clone();
    let untranslated_issues_summary = untranslated_issues.clone();
    let parse_errors = ctx.parsed_files_errors();

    let mut all_issues: Vec<Issue> = Vec::new();
    all_issues.extend(hardcoded_issues.into_iter().map(Issue::HardcodedText));
    all_issues.extend(untranslated_issues.into_iter().map(Issue::Untranslated));
    all_issues.extend(parse_errors.iter().map(|i| Issue::ParseError(i.clone())));

    Ok(finish(
        CommandKind::Baseline,
        CommandSummary::Baseline(BaselineSummary {
            hardcoded_count,
            untranslated_usage_count,
            untranslated_key_count,
            file_count,
            is_apply: apply,
            hardcoded_issues: hardcoded_issues_summary,
            untranslated_issues: untranslated_issues_summary,
        }),
        all_issues,
        ctx.files.len(),
        ctx.messages().all_messages.len(),
    ))
}

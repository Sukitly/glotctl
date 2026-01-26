use std::collections::HashSet;

use super::super::{
    actions::{Action, ActionStats, DeleteKey},
    args::{CleanCommand, CleanRule},
};
use super::helper::finish;
use super::{CleanSummary, CommandResult, CommandSummary};
use crate::{
    core::CheckContext,
    issues::{Issue, OrphanKeyIssue, UnusedKeyIssue},
    rules::{orphan::check_orphan_keys_issues, unused::check_unused_keys_issues},
};
use anyhow::{Ok, Result};

impl CleanRule {
    pub fn all() -> HashSet<Self> {
        [Self::Unused, Self::Orphan].into_iter().collect()
    }
}

pub fn clean(cmd: CleanCommand) -> Result<CommandResult> {
    let args = &cmd.args;
    let ctx = CheckContext::new(&args.common.path, args.common.verbose)?;
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

    let file_count = if apply {
        let mut stats = ActionStats::default();
        if !unused_issues.is_empty() {
            stats += DeleteKey::run(&unused_issues)?;
        }
        if !orphan_issues.is_empty() {
            stats += DeleteKey::run(&orphan_issues)?;
        }
        stats.files_modified
    } else {
        let mut files: HashSet<&str> = HashSet::new();
        for issue in &unused_issues {
            files.insert(issue.context.file_path());
        }
        for issue in &orphan_issues {
            files.insert(issue.context.file_path());
        }
        files.len()
    };

    let unused_issues_summary = unused_issues.clone();
    let orphan_issues_summary = orphan_issues.clone();
    let parse_errors = ctx.parsed_files_errors();

    let mut all_issues: Vec<Issue> = Vec::new();
    all_issues.extend(unused_issues.into_iter().map(Issue::UnusedKey));
    all_issues.extend(orphan_issues.into_iter().map(Issue::OrphanKey));
    all_issues.extend(parse_errors.iter().map(|i| Issue::ParseError(i.clone())));

    Ok(finish(
        CommandSummary::Clean(CleanSummary {
            unused_count,
            orphan_count,
            file_count,
            is_apply: apply,
            unused_issues: unused_issues_summary,
            orphan_issues: orphan_issues_summary,
        }),
        all_issues,
        ctx.files.len(),
        ctx.messages().all_messages.len(),
        false,
    ))
}

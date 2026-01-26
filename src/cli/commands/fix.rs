use std::collections::HashSet;

use crate::{
    cli::actions::{Action, ActionStats, InsertMessageKeys},
    cli::args::FixCommand,
    cli::commands::helper::finish,
    cli::commands::{CommandResult, CommandSummary, FixSummary},
    core::CheckContext,
    issues::{Issue, UnresolvedKeyIssue},
    rules::unresolved::check_unresolved_keys_issues,
};
use anyhow::{Ok, Result};

pub fn fix(cmd: FixCommand) -> Result<CommandResult> {
    let args = &cmd.args;
    let ctx = CheckContext::new(&args.common.path, args.common.verbose)?;
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

    let unresolved_issues_summary = unresolved_issues.clone();
    let parse_errors = ctx.parsed_files_errors();

    let mut all_issues: Vec<Issue> = Vec::new();
    all_issues.extend(unresolved_issues.into_iter().map(Issue::UnresolvedKey));
    all_issues.extend(parse_errors.iter().map(|i| Issue::ParseError(i.clone())));

    Ok(finish(
        CommandSummary::Fix(FixSummary {
            unresolved_count,
            processed_count,
            applied_count,
            skipped_count,
            file_count,
            is_apply: apply,
            unresolved_issues: unresolved_issues_summary,
        }),
        all_issues,
        ctx.files.len(),
        ctx.messages().all_messages.len(),
        false,
    ))
}

use anyhow::{Ok, Result};
use clap::ValueEnum;

use crate::{
    args::CheckCommand,
    commands::RunResult,
    commands::{context::CheckContext, helper::finish},
    issues::Issue,
    report::report,
    rules::{
        hardcoded::check_hardcoded_issues, missing::check_missing_keys_issues,
        orphan::check_orphan_keys_issues, replica_lag::check_replica_lag_issues,
        type_mismatch::check_type_mismatch_issues, unresolved::check_unresolved_keys_issues,
        untranslated::check_untranslated_issues, unused::check_unused_keys_issues,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub enum CheckRule {
    Hardcoded,
    Missing,
    Unused,
    Orphan,
    ReplicaLag,
    Untranslated,
    TypeMismatch,
    Unresolved,
}

impl CheckRule {
    pub fn default() -> Vec<CheckRule> {
        vec![
            CheckRule::Hardcoded,
            CheckRule::Missing,
            CheckRule::Unused,
            CheckRule::Orphan,
            CheckRule::ReplicaLag,
            CheckRule::Untranslated,
            CheckRule::TypeMismatch,
            CheckRule::Unresolved,
        ]
    }
}

pub fn check(cmd: CheckCommand) -> Result<RunResult> {
    let args = &cmd.args;
    let checks = &cmd.checks;
    let ctx = CheckContext::new(&args.common)?;

    let checks = if checks.is_empty() {
        CheckRule::default()
    } else {
        checks.clone()
    };

    let mut all_issues: Vec<Issue> = Vec::new();

    for check in checks {
        match check {
            CheckRule::Hardcoded => {
                let issues = check_hardcoded_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::Hardcoded));
            }
            CheckRule::Missing => {
                let issues = check_missing_keys_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::MissingKey));
            }
            CheckRule::Unused => {
                let issues = check_unused_keys_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::UnusedKey));
            }
            CheckRule::Orphan => {
                let issues = check_orphan_keys_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::OrphanKey));
            }
            CheckRule::ReplicaLag => {
                let issues = check_replica_lag_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::ReplicaLag));
            }
            CheckRule::Untranslated => {
                let issues = check_untranslated_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::Untranslated));
            }
            CheckRule::TypeMismatch => {
                let issues = check_type_mismatch_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::TypeMismatch));
            }
            CheckRule::Unresolved => {
                let issues = check_unresolved_keys_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::UnresolvedKey));
            }
        }
    }

    let parse_errors = ctx.parsed_files_errors();
    all_issues.extend(parse_errors.iter().map(|i| Issue::ParseError(i.clone())));

    report(&all_issues);

    Ok(finish(
        all_issues,
        ctx.files.len(),
        ctx.messages().all_messages.len(),
    ))
}

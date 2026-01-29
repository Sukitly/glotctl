//! Check command - Run i18n validation checks.
//!
//! This command runs various i18n checks on the codebase:
//! - `hardcoded`: Detect hardcoded text that should be translated
//! - `missing`: Find translation keys used in code but not in message files
//! - `unused`: Find keys in message files that are never used
//! - `orphan`: Find keys in message files that don't exist in primary locale
//! - `replica-lag`: Find keys missing in non-primary locales
//! - `untranslated`: Find keys with untranslated values (same as English)
//! - `type-mismatch`: Find keys with mismatched value types across locales
//! - `unresolved`: Find dynamic keys that couldn't be statically resolved
//!
//! By default, all checks are run. You can specify specific checks to run.

use anyhow::Result;
use clap::ValueEnum;

use super::super::args::CheckCommand;
use super::super::exit_status::ExitStatus;
use super::super::report;

use crate::{
    core::CheckContext,
    issues::{Issue, Severity},
    rules::{
        hardcoded::check_hardcoded_text_issues, missing::check_missing_keys_issues,
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
    pub fn all() -> Vec<CheckRule> {
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

pub fn check(cmd: CheckCommand, verbose: bool) -> Result<ExitStatus> {
    let args = &cmd.args;
    let checks = &cmd.checks;
    let ctx = CheckContext::new(&args.common)?;

    let checks = if checks.is_empty() {
        CheckRule::all()
    } else {
        checks.clone()
    };

    let mut all_issues: Vec<Issue> = Vec::new();

    for check in checks {
        match check {
            CheckRule::Hardcoded => {
                let issues = check_hardcoded_text_issues(&ctx);
                all_issues.extend(issues.into_iter().map(Issue::HardcodedText));
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
    all_issues.sort();

    let parse_error_count = parse_errors.len();
    let has_errors = all_issues.iter().any(|i| i.severity() == Severity::Error);

    // Print output
    if all_issues.is_empty() {
        report::print_no_issue(ctx.files.len(), ctx.messages().all_messages.len());
    } else {
        report::report(&all_issues);
    }
    report::print_parse_error(parse_error_count, verbose);

    // Determine exit status
    if parse_error_count > 0 {
        Ok(ExitStatus::Error)
    } else if has_errors {
        Ok(ExitStatus::Failure)
    } else {
        Ok(ExitStatus::Success)
    }
}

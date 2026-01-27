use super::{CommandResult, CommandSummary};
use crate::issues::{Issue, Severity};

pub fn finish(
    summary: CommandSummary,
    mut issues: Vec<Issue>,
    source_files_checked: usize,
    locale_files_checked: usize,
    exit_on_errors: bool,
) -> CommandResult {
    issues.sort();

    let parse_error_count = issues
        .iter()
        .filter(|i| matches!(i, Issue::ParseError(_)))
        .count();

    let mut error_count = issues
        .iter()
        .filter(|i| i.severity() == Severity::Error)
        .count();

    if let CommandSummary::Init(ref summary) = summary
        && summary.error.is_some()
    {
        error_count += 1;
    }

    CommandResult {
        summary,
        error_count,
        exit_on_errors,
        issues,
        parse_error_count,
        source_files_checked,
        locale_files_checked,
    }
}

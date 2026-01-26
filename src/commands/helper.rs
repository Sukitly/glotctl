use crate::{
    commands::RunResult,
    issues::{Issue, Severity},
};

pub fn finish(
    mut issues: Vec<Issue>,
    source_files_checked: usize,
    locale_files_checked: usize,
) -> RunResult {
    issues.sort();

    let parse_error_count = issues
        .iter()
        .filter(|i| matches!(i, Issue::ParseError(_)))
        .count();

    let error_count = issues
        .iter()
        .filter(|i| i.severity() == Severity::Error)
        .count();

    let warning_count = issues
        .iter()
        .filter(|i| i.severity() == Severity::Warning)
        .count();

    RunResult {
        error_count,
        warning_count,
        exit_on_errors: true,
        issues,
        parse_error_count,
        source_files_checked,
        locale_files_checked,
    }
}

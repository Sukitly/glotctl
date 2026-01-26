use crate::issues::Issue;

/// Result of running glot commands
pub struct RunResult {
    pub error_count: usize,
    pub warning_count: usize,
    /// If true, exit code 1 should be returned when error_count > 0.
    /// If false, always exit 0 (used for dry-run commands that report work to do).
    pub exit_on_errors: bool,
    /// All issues found during the check.
    /// Empty for non-check commands.
    pub issues: Vec<Issue>,
    /// Number of files that failed to parse.
    pub parse_error_count: usize,
    /// Number of source files (TSX/JSX) that were checked.
    pub source_files_checked: usize,
    /// Number of locale message files (JSON) that were checked.
    /// 0 if message checking was not performed.
    pub locale_files_checked: usize,
}

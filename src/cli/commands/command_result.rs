use crate::issues::Issue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Check,
    Baseline,
    Fix,
    Clean,
    Init,
}

#[derive(Debug)]
pub enum CommandSummary {
    Check,
    Baseline(BaselineSummary),
    Fix(FixSummary),
    Clean(CleanSummary),
    Init(InitSummary),
}

#[derive(Debug)]
pub struct BaselineSummary {
    pub hardcoded_count: usize,
    pub untranslated_usage_count: usize,
    pub untranslated_key_count: usize,
    pub file_count: usize,
    pub is_apply: bool,
    pub hardcoded_issues: Vec<crate::issues::HardcodedTextIssue>,
    pub untranslated_issues: Vec<crate::issues::UntranslatedIssue>,
}

#[derive(Debug)]
pub struct FixSummary {
    pub unresolved_count: usize,
    pub inserted_count: usize,
    pub skipped_count: usize,
    pub file_count: usize,
    pub is_apply: bool,
    pub unresolved_issues: Vec<crate::issues::UnresolvedKeyIssue>,
}

#[derive(Debug)]
pub struct CleanSummary {
    pub unused_count: usize,
    pub orphan_count: usize,
    pub file_count: usize,
    pub is_apply: bool,
    pub unused_issues: Vec<crate::issues::UnusedKeyIssue>,
    pub orphan_issues: Vec<crate::issues::OrphanKeyIssue>,
}

#[derive(Debug)]
pub struct InitSummary {
    pub created: bool,
}

/// Result of running glot commands
pub struct CommandResult {
    pub kind: CommandKind,
    pub summary: CommandSummary,
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

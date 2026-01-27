use std::process::ExitCode;

/// Exit status for CLI commands, following common conventions for linter tools.
///
/// - `Success` (0): Command completed successfully, no issues found
/// - `Failure` (1): Command completed but found issues (errors/warnings)
/// - `Error` (2): Command failed due to internal error (parse error, config error, etc.)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExitStatus {
    /// Command completed successfully, no issues found.
    Success,
    /// Command completed but found issues (errors/warnings).
    Failure,
    /// Command failed due to internal error (parse error, config error, etc.).
    Error,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => ExitCode::from(0),
            ExitStatus::Failure => ExitCode::from(1),
            ExitStatus::Error => ExitCode::from(2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_values() {
        assert_eq!(ExitCode::from(ExitStatus::Success), ExitCode::from(0));
        assert_eq!(ExitCode::from(ExitStatus::Failure), ExitCode::from(1));
        assert_eq!(ExitCode::from(ExitStatus::Error), ExitCode::from(2));
    }
}

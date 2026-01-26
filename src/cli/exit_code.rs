use super::commands::CommandResult;

pub fn exit_code_from_result(result: &CommandResult) -> i32 {
    if result.parse_error_count > 0 {
        1
    } else if result.exit_on_errors && result.error_count > 0 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands::{CommandResult, CommandSummary, InitSummary};

    fn result_with(
        error_count: usize,
        parse_error_count: usize,
        exit_on_errors: bool,
    ) -> CommandResult {
        CommandResult {
            summary: CommandSummary::Init(InitSummary { created: true }),
            error_count,
            exit_on_errors,
            issues: Vec::new(),
            parse_error_count,
            source_files_checked: 0,
            locale_files_checked: 0,
        }
    }

    #[test]
    fn exit_code_is_non_zero_for_parse_errors() {
        let result = result_with(0, 1, false);
        assert_eq!(exit_code_from_result(&result), 1);
    }

    #[test]
    fn exit_code_is_zero_when_errors_ignored() {
        let result = result_with(2, 0, false);
        assert_eq!(exit_code_from_result(&result), 0);
    }

    #[test]
    fn exit_code_is_non_zero_when_errors_enforced() {
        let result = result_with(1, 0, true);
        assert_eq!(exit_code_from_result(&result), 1);
    }
}

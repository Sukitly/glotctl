use super::commands::CommandResult;

pub fn exit_code_from_result(result: &CommandResult) -> i32 {
    if result.exit_on_errors && result.error_count > 0 {
        1
    } else {
        0
    }
}

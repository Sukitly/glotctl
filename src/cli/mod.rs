use anyhow::Result;

use crate::args::Arguments;
use crate::cli::exit_code::exit_code_from_result;

mod exit_code;
mod report;
mod run;

pub fn run_cli(args: Arguments) -> Result<i32> {
    let verbose = args.verbose();

    let Some(args) = args.with_command_or_help() else {
        return Ok(0);
    };

    let result = run::run(args)?;
    report::print(&result, verbose);

    Ok(exit_code_from_result(&result))
}

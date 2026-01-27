use std::process::ExitCode;

use anyhow::Result;

pub use args::{Arguments, Command};
pub use exit_status::ExitStatus;

mod actions;
pub mod args;
mod commands;
mod exit_status;
pub mod report;
mod run;

pub fn run_cli(args: Arguments) -> Result<ExitCode> {
    let Some(args) = args.with_command_or_help() else {
        return Ok(ExitCode::from(0));
    };

    let status = run::run(args)?;
    Ok(status.into())
}

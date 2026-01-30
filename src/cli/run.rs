/// Main entry point for the glot CLI.
///
/// Dispatches to the appropriate command handler based on the parsed arguments.
/// Each command handles its own console output and returns an ExitStatus.
use std::time::Instant;

use super::{
    args::{Arguments, Command},
    commands::{baseline, check, clean, fix, init},
    exit_status::ExitStatus,
    report,
};
use anyhow::Result;

pub fn run(args: Arguments) -> Result<ExitStatus> {
    let verbose = args.verbose();

    match args.command {
        Some(Command::Check(cmd)) => {
            let start = Instant::now();
            let result = check::check(cmd, verbose)?;
            report::print_execution_time(start.elapsed());
            Ok(result)
        }
        Some(Command::Clean(cmd)) => {
            let start = Instant::now();
            let result = clean::clean(cmd, verbose)?;
            report::print_execution_time(start.elapsed());
            Ok(result)
        }
        Some(Command::Baseline(cmd)) => {
            let start = Instant::now();
            let result = baseline::baseline(cmd, verbose)?;
            report::print_execution_time(start.elapsed());
            Ok(result)
        }
        Some(Command::Fix(cmd)) => {
            let start = Instant::now();
            let result = fix::fix(cmd, verbose)?;
            report::print_execution_time(start.elapsed());
            Ok(result)
        }
        Some(Command::Init) => init::init(),
        Some(Command::Serve) => {
            // Serve command is handled in main.rs before calling run()
            anyhow::bail!("Serve command should be handled before run()")
        }
        None => {
            anyhow::bail!("No command provided. Use --help to see available commands.")
        }
    }
}

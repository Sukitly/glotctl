/// Main entry point for the glot CLI.
///
/// Dispatches to the appropriate command handler based on the parsed arguments.
/// Each command handles its own console output and returns an ExitStatus.
use super::{
    args::{Arguments, Command},
    commands::{baseline, check, clean, fix, init},
    exit_status::ExitStatus,
};
use anyhow::Result;

pub fn run(args: Arguments) -> Result<ExitStatus> {
    let verbose = args.verbose();

    match args.command {
        Some(Command::Check(cmd)) => check::check(cmd, verbose),
        Some(Command::Clean(cmd)) => clean::clean(cmd, verbose),
        Some(Command::Baseline(cmd)) => baseline::baseline(cmd, verbose),
        Some(Command::Fix(cmd)) => fix::fix(cmd, verbose),
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

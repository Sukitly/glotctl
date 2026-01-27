/// Main entry point for the glot CLI.
///
/// Dispatches to the appropriate command handler based on the parsed arguments.
///
/// # Returns
/// - `Ok(CommandResult)` with error/warning counts and exit behavior
/// - `Err` if the command fails (e.g., config not found, parse errors)
///
/// # Example
/// ```ignore
/// let args = Arguments::parse();
/// let result = glot::run(args)?;
/// if result.exit_on_errors && result.error_count > 0 {
///     std::process::exit(1);
/// }
/// ```
use super::{
    args::{Arguments, Command},
    commands::{
        CommandResult, baseline::baseline, check::check, clean::clean, fix::fix, init::init,
    },
};
use anyhow::Result;

pub fn run(Arguments { command }: Arguments) -> Result<CommandResult> {
    match command {
        Some(Command::Check(cmd)) => check(cmd),
        Some(Command::Clean(cmd)) => clean(cmd),
        Some(Command::Baseline(cmd)) => baseline(cmd),
        Some(Command::Fix(cmd)) => fix(cmd),
        Some(Command::Init) => init(),
        Some(Command::Serve) => {
            // Serve command is handled in main.rs before calling run()
            anyhow::bail!("Serve command should be handled before run()")
        }
        None => {
            anyhow::bail!("No command provided. Use --help to see available commands.")
        }
    }
}

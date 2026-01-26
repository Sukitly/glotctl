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
use std::{fs, path::Path};

use super::{
    args::{Arguments, Command},
    commands::{CommandResult, CommandSummary, InitSummary},
    commands::{baseline::baseline, check::check, clean::clean, fix::fix},
};
use crate::config::{CONFIG_FILE_NAME, default_config_json};
use anyhow::Result;

pub fn run(Arguments { command }: Arguments) -> Result<CommandResult> {
    match command {
        Some(Command::Check(cmd)) => check(cmd),
        Some(Command::Clean(cmd)) => clean(cmd),
        Some(Command::Baseline(cmd)) => baseline(cmd),
        Some(Command::Fix(cmd)) => fix(cmd),
        Some(Command::Init) => {
            init()?;
            Ok(CommandResult {
                summary: CommandSummary::Init(InitSummary { created: true }),
                error_count: 0,
                exit_on_errors: true,
                issues: Vec::new(),
                parse_error_count: 0,
                source_files_checked: 0,
                locale_files_checked: 0,
            })
        }
        Some(Command::Serve) => {
            // Serve command is handled in main.rs before calling run()
            anyhow::bail!("Serve command should be handled before run()")
        }
        None => {
            anyhow::bail!("No command provided. Use --help to see available commands.")
        }
    }
}

fn init() -> Result<()> {
    let config_path = Path::new(CONFIG_FILE_NAME);
    if config_path.exists() {
        anyhow::bail!("{} already exists", CONFIG_FILE_NAME);
    }

    fs::write(config_path, default_config_json()?)?;
    Ok(())
}

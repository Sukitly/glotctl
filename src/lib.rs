use std::{fs, path::Path};

use colored::Colorize;

use crate::{
    args::{Arguments, BaselineCommand, CheckCommand, CleanCommand, Command, FixCommand},
    commands::{baseline::BaselineRunner, clean::CleanRunner, fix::FixRunner, runner::CheckRunner},
    config::{CONFIG_FILE_NAME, default_config_json},
    issue::Issue,
    reporter::SUCCESS_MARK,
};
use anyhow::Result;

/// Result of running glot commands
pub struct RunResult {
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

pub mod args;
pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod directives;
pub(crate) mod extraction;
pub(crate) mod file_scanner;
pub mod issue;
pub(crate) mod json_editor;
pub(crate) mod json_writer;
pub mod mcp;
pub(crate) mod parsers;
pub mod reporter;
pub(crate) mod rules;
pub mod utils;

/// Main entry point for the glot CLI.
///
/// Dispatches to the appropriate command handler based on the parsed arguments.
///
/// # Returns
/// - `Ok(RunResult)` with error/warning counts and exit behavior
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
pub fn run(Arguments { command }: Arguments) -> Result<RunResult> {
    match command {
        Some(Command::Check(cmd)) => check(cmd),
        Some(Command::Clean(cmd)) => clean(cmd),
        Some(Command::Baseline(cmd)) => baseline(cmd),
        Some(Command::Fix(cmd)) => fix(cmd),
        Some(Command::Init) => {
            init()?;
            Ok(RunResult {
                error_count: 0,
                warning_count: 0,
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

fn check(cmd: CheckCommand) -> Result<RunResult> {
    let mut runner = CheckRunner::new(cmd.args)?;

    if cmd.checks.is_empty() {
        runner = runner.all();
    } else {
        for check_type in cmd.checks {
            runner = runner.add(check_type);
        }
    }

    runner.run()
}

fn clean(cmd: CleanCommand) -> Result<RunResult> {
    CleanRunner::new(cmd.args)?.run()
}

fn baseline(cmd: BaselineCommand) -> Result<RunResult> {
    BaselineRunner::new(cmd.args)?.run()
}

fn fix(cmd: FixCommand) -> Result<RunResult> {
    FixRunner::new(cmd.args)?.run()
}

fn init() -> Result<()> {
    let config_path = Path::new(CONFIG_FILE_NAME);
    if config_path.exists() {
        anyhow::bail!("{} already exists", CONFIG_FILE_NAME);
    }

    fs::write(config_path, default_config_json()?)?;
    println!(
        "{} {}",
        SUCCESS_MARK.green(),
        format!("Created {}", CONFIG_FILE_NAME).green()
    );

    Ok(())
}

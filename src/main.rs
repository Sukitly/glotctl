//! Glot CLI application entry point.
//!
//! This binary handles command dispatch for the Glot i18n checker tool.
//! It routes to either the CLI commands or the MCP server based on the
//! command-line arguments.

use std::process::ExitCode;

use clap::Parser;
use glot::cli::{Arguments, Command, ExitStatus};

fn main() -> ExitCode {
    let args = Arguments::parse();

    if matches!(args.command, Some(Command::Serve)) {
        if let Err(err) = glot::mcp::run_server() {
            eprintln!("Error: {}", err);
            return ExitStatus::Error.into();
        }
        return ExitStatus::Success.into();
    }

    match glot::cli::run_cli(args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Error: {}", err);
            ExitStatus::Error.into()
        }
    }
}

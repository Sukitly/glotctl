use anyhow::Result;
use clap::Parser;
use glot::args::{Arguments, Command};

fn main() -> Result<()> {
    let args = Arguments::parse();

    // Handle MCP serve command early (requires async runtime)
    if matches!(args.command, Some(Command::Serve)) {
        return glot::mcp::run_server();
    }

    let verbose = args.verbose();

    // If no command provided, print help and exit
    let Some(args) = args.with_command_or_help() else {
        return Ok(());
    };

    let result = glot::run(args)?;

    // Print report for check commands
    if !result.issues.is_empty() {
        glot::reporter::print_report(&result.issues);
    } else if result.source_files_checked > 0 {
        // Print success message for check commands with no issues
        glot::reporter::print_success(result.source_files_checked, result.locale_files_checked);
    }

    // Print parse warning if needed
    glot::reporter::print_parse_warning(result.parse_error_count, verbose);

    if result.exit_on_errors && result.error_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

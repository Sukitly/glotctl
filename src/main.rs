use clap::Parser;
use glot::cli::{Arguments, Command};

fn main() {
    let args = Arguments::parse();

    if matches!(args.command, Some(Command::Serve)) {
        if let Err(err) = glot::mcp::run_server() {
            eprintln!("Error: {}", err);
            std::process::exit(2);
        }
        return;
    }

    match glot::cli::run_cli(args) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    }
}

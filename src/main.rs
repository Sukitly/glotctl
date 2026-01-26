use clap::Parser;
use glot::args::Arguments;

fn main() {
    let args = Arguments::parse();

    // Handle MCP serve command early (requires async runtime)
    // if matches!(args.command, Some(Command::Serve)) {
    //     return glot::mcp::run_server();
    // }

    match glot::cli::run_cli(args) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(2);
        }
    }
}

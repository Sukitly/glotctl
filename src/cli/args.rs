//! CLI argument definitions using clap.
//!
//! This module defines the command-line interface structure for all Glot commands.
//! It uses clap's derive API for declarative argument parsing.
//!
//! ## Commands
//!
//! - `check`: Run i18n checks (hardcoded text, missing keys, etc.)
//! - `clean`: Remove unused/orphan keys from message files
//! - `baseline`: Add disable comments to suppress existing issues
//! - `fix`: Fix unresolved key issues with glot-message-keys comments
//! - `init`: Initialize glot configuration file
//! - `serve`: Start MCP server for AI integration

use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};

use super::commands::check::CheckRule;
use crate::core::collect::SuppressibleRule;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Arguments {
    /// Check if a command was provided, otherwise print help and return None.
    pub fn with_command_or_help(self) -> Option<Self> {
        if self.command.is_none() {
            Self::command().print_help().ok();
            None
        } else {
            Some(self)
        }
    }

    /// Get the verbose flag from the command's common args.
    pub fn verbose(&self) -> bool {
        match &self.command {
            Some(Command::Check(cmd)) => cmd.args.common.verbose,
            Some(Command::Clean(cmd)) => cmd.args.common.verbose,
            Some(Command::Baseline(cmd)) => cmd.args.common.verbose,
            Some(Command::Fix(cmd)) => cmd.args.common.verbose,
            Some(Command::Init) | Some(Command::Serve) | None => false,
        }
    }
}

/// Common arguments shared by all commands.
#[derive(Debug, Clone, Args)]
pub struct CommonArgs {
    /// Primary locale (overrides config file)
    #[arg(long)]
    pub primary_locale: Option<String>,

    /// Source code root directory (overrides config file)
    #[arg(long)]
    pub source_root: Option<PathBuf>,

    /// Messages directory path (overrides config file)
    #[arg(long)]
    pub messages_root: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Parser)]
pub struct CheckArgs {
    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, Args)]
pub struct CheckCommand {
    #[arg(value_enum)]
    pub checks: Vec<CheckRule>,
    #[command(flatten)]
    pub args: CheckArgs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub enum CleanRule {
    Unused,
    Orphan,
}
#[derive(Debug, Parser)]
pub struct CleanArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// Actually delete keys (default is dry-run)
    #[arg(long)]
    pub apply: bool,

    /// Rules to clean (default: all)
    /// Can be specified multiple times: --rules unused --rules orphan
    #[arg(long, value_enum)]
    pub rules: Vec<CleanRule>,
}

#[derive(Debug, Args)]
pub struct CleanCommand {
    #[command(flatten)]
    pub args: CleanArgs,
}

#[derive(Debug, Parser)]
pub struct BaselineArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// Actually insert comments (default is dry-run)
    #[arg(long)]
    pub apply: bool,

    /// Rules to add disable comments for (default: all)
    /// Can be specified multiple times: --rule hardcoded --rule untranslated
    #[arg(long, value_enum)]
    pub rules: Vec<SuppressibleRule>,
}

#[derive(Debug, Args)]
pub struct BaselineCommand {
    #[command(flatten)]
    pub args: BaselineArgs,
}

#[derive(Debug, Parser)]
pub struct FixArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// Actually insert comments (default is dry-run)
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct FixCommand {
    #[command(flatten)]
    pub args: FixArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check for i18n issues (hardcoded text, missing keys, orphan keys, untranslated values)
    Check(CheckCommand),
    /// Remove unused or orphan translation keys from JSON files
    Clean(CleanCommand),
    /// Insert glot-disable-next-line comments to suppress hardcoded text warnings
    Baseline(BaselineCommand),
    /// Insert glot-message-keys comments for dynamic translation keys
    Fix(FixCommand),
    /// Initialize a new .glotrc.json configuration file
    Init,
    /// Start MCP server for AI coding agents
    Serve,
}

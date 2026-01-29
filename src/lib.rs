//! Glot - Next.js i18n checker for next-intl
//!
//! Glot is a CLI tool and library for checking internationalization (i18n) issues
//! in Next.js projects using next-intl. It detects hardcoded text, missing translation
//! keys, unused keys, and other i18n-related issues.
//!
//! ## Module Structure
//!
//! - `cli`: Command-line interface layer (user-facing commands and actions)
//! - `config`: Configuration file loading and parsing
//! - `core`: Core analysis engine (three-phase pipeline)
//! - `issues`: Issue type definitions and reporting
//! - `mcp`: Model Context Protocol server implementation
//! - `rules`: Detection rules for various i18n issues
//! - `utils`: Shared utility functions

pub mod cli;
pub mod config;
pub mod core;
pub mod issues;
pub mod mcp;
pub mod rules;
pub mod utils;

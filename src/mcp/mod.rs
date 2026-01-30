//! Model Context Protocol (MCP) server implementation.
//!
//! This module provides an MCP server that exposes Glot functionality to AI assistants
//! like Claude Desktop. The server implements the MCP specification for tool calling.
//!
//! ## Module Structure
//!
//! - `helpers`: Helper functions for MCP message handling
//! - `json_writer`: JSON-RPC message writer
//! - `server`: Main MCP server implementation
//! - `types`: MCP-specific type definitions

mod helpers;
mod json_writer;
mod server;
pub mod types;

pub use server::{GlotMcpServer, run_server};

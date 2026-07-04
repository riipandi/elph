use clap::{Parser, Subcommand};

use super::help;
use crate::runtime::{EXIT_SUCCESS, ExitCode};

#[derive(Parser, Default)]
#[command(
    name = "mcp",
    about = "Manage MCP server configurations",
    color = clap::ColorChoice::Auto
)]
pub struct McpArgs {
    #[command(subcommand)]
    pub command: Option<McpCommands>,
}

#[derive(Subcommand)]
pub enum McpCommands {
    /// List configured MCP servers
    List,
    /// Add or update an MCP server configuration
    Add {
        /// Name of the MCP server
        name: String,
        /// MCP server configuration (JSON string or file path)
        #[arg(value_name = "CONFIG")]
        config: Option<String>,
    },
    /// Remove an MCP server configuration
    Remove {
        /// Name of the MCP server to remove
        name: String,
    },
    /// Diagnose MCP server configuration and connectivity
    Doctor,
    /// Authenticate with an OAuth-enabled MCP server
    Auth {
        /// Name of the MCP server
        name: String,
    },
    /// Remove OAuth credentials for an MCP server
    Logout {
        /// Name of the MCP server
        name: String,
    },
}

pub fn handle(args: &McpArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<McpArgs>();
    };
    match cmd {
        McpCommands::List => {
            help::unimplemented("MCP list — not yet implemented");
            EXIT_SUCCESS
        }
        McpCommands::Add { name, config } => {
            help::unimplemented(&format!(
                "MCP add — not yet implemented (name: {name}, config: {})",
                config.as_deref().unwrap_or("<interactive>")
            ));
            EXIT_SUCCESS
        }
        McpCommands::Remove { name } => {
            help::unimplemented(&format!("MCP remove — not yet implemented (name: {name})"));
            EXIT_SUCCESS
        }
        McpCommands::Doctor => {
            help::unimplemented("MCP doctor — not yet implemented");
            EXIT_SUCCESS
        }
        McpCommands::Auth { name } => {
            help::unimplemented(&format!("MCP auth — not yet implemented (name: {name})"));
            EXIT_SUCCESS
        }
        McpCommands::Logout { name } => {
            help::unimplemented(&format!("MCP logout — not yet implemented (name: {name})"));
            EXIT_SUCCESS
        }
    }
}

use clap::{Args, Subcommand};

use crate::runtime::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
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
        eprintln!("Manage MCP server configurations");
        eprintln!();
        eprintln!("Usage: elph mcp <command>");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  list    List configured MCP servers");
        eprintln!("  add     Add or update an MCP server configuration");
        eprintln!("  remove  Remove an MCP server configuration");
        eprintln!("  doctor  Diagnose MCP server configuration and connectivity");
        eprintln!("  auth    Authenticate with an OAuth-enabled MCP server");
        eprintln!("  logout  Remove OAuth credentials for an MCP server");
        eprintln!("  help    Print this message or the help of the given subcommand(s)");
        return EXIT_SUCCESS;
    };
    match cmd {
        McpCommands::List => {
            eprintln!("MCP list — not yet implemented");
            EXIT_SUCCESS
        }
        McpCommands::Add { name, config } => {
            eprintln!(
                "MCP add — not yet implemented (name: {name}, config: {})",
                config.as_deref().unwrap_or("<interactive>")
            );
            EXIT_SUCCESS
        }
        McpCommands::Remove { name } => {
            eprintln!("MCP remove — not yet implemented (name: {name})");
            EXIT_SUCCESS
        }
        McpCommands::Doctor => {
            eprintln!("MCP doctor — not yet implemented");
            EXIT_SUCCESS
        }
        McpCommands::Auth { name } => {
            eprintln!("MCP auth — not yet implemented (name: {name})");
            EXIT_SUCCESS
        }
        McpCommands::Logout { name } => {
            eprintln!("MCP logout — not yet implemented (name: {name})");
            EXIT_SUCCESS
        }
    }
}

use clap::{Args, Subcommand};

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

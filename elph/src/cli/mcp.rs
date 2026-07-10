use clap::{Parser, Subcommand};

use super::help;
use elph_core::utils::path::AppPaths;

use crate::platform::mcp as mcp_runtime;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths, Settings, ensure_home_blocking};
use elph_agent::probe_server;

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

    let paths = match ensure_home_blocking(env!("CARGO_PKG_VERSION")) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };

    match cmd {
        McpCommands::List => match mcp_runtime::load_config(&paths) {
            Ok(config) => {
                if config.servers.is_empty() {
                    println!("No MCP servers configured.");
                    println!("Config: {}", paths.mcp_config_path().display());
                } else {
                    for (name, server) in &config.servers {
                        println!("{name}: {server:?}");
                    }
                }
                EXIT_SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                EXIT_ERROR
            }
        },
        McpCommands::Add { name, config } => {
            let Some(raw) = config else {
                help::unimplemented("MCP add — interactive config entry not yet implemented");
                return EXIT_SUCCESS;
            };
            match mcp_runtime::parse_server_config(raw) {
                Ok(server) => match mcp_runtime::upsert_server(&paths, name, server) {
                    Ok(()) => {
                        let _ = mcp_runtime::ensure_project_mcp_cache(&paths);
                        println!("Saved MCP server '{name}' to {}", paths.mcp_config_path().display());
                        EXIT_SUCCESS
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        EXIT_ERROR
                    }
                },
                Err(error) => {
                    eprintln!("{error}");
                    EXIT_ERROR
                }
            }
        }
        McpCommands::Remove { name } => match mcp_runtime::remove_server(&paths, name) {
            Ok(true) => {
                println!("Removed MCP server '{name}'.");
                EXIT_SUCCESS
            }
            Ok(false) => {
                eprintln!("MCP server '{name}' not found.");
                EXIT_ERROR
            }
            Err(error) => {
                eprintln!("{error}");
                EXIT_ERROR
            }
        },
        McpCommands::Doctor => handle_doctor(&paths),
        McpCommands::Auth { name } => {
            help::unimplemented(&format!("MCP auth — OAuth via rmcp not yet implemented (name: {name})"));
            EXIT_SUCCESS
        }
        McpCommands::Logout { name } => {
            help::unimplemented(&format!("MCP logout — not yet implemented (name: {name})"));
            EXIT_SUCCESS
        }
    }
}

fn handle_doctor(paths: &Paths) -> ExitCode {
    let settings = match Settings::load(paths) {
        Ok(s) => s,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    let _ = settings;

    let config = match mcp_runtime::load_config(paths) {
        Ok(c) => c,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };

    if config.servers.is_empty() {
        println!("No MCP servers configured.");
        return EXIT_SUCCESS;
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut ok = true;
    for (name, server) in &config.servers {
        let result = rt.block_on(probe_server(name, server));
        let status = if result.ok { "ok" } else { "fail" };
        println!("{name}: {status} — {}", result.message);
        if !result.ok {
            ok = false;
        }
    }

    if let Ok(cache) = mcp_runtime::project_mcp_cache_dir(paths) {
        println!("Project MCP cache: {}", cache.display());
    }

    if ok { EXIT_SUCCESS } else { EXIT_ERROR }
}

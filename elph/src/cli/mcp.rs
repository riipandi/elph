use clap::{Parser, Subcommand};

use super::help;
use crate::platform::mcp as mcp_runtime;
use crate::platform::mcp::{McpConfigScope, McpServerSource};
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths, Settings, ensure_home_blocking};
use elph_agent::{McpAuthSourceReport, McpOAuthFlowOptions, McpServerConfig};
use elph_agent::{clear_credentials, has_stored_credentials, probe_server_with_auth, run_oauth_flow};
use elph_core::utils::path::AppPaths;

#[derive(Parser, Default)]
#[command(
    name = "mcp",
    about = "Manage MCP server configurations (home + project layers)",
    color = clap::ColorChoice::Auto
)]
pub struct McpArgs {
    #[command(subcommand)]
    pub command: Option<McpCommands>,
}

#[derive(Subcommand)]
pub enum McpCommands {
    /// List configured MCP servers (merged home + project)
    List {
        /// Show only the project layer (`<project>/.elph/mcp.json`)
        #[arg(long)]
        project: bool,
        /// Show only the home layer (`~/.elph/mcp.json`)
        #[arg(long)]
        home: bool,
    },
    /// Add or update an MCP server configuration
    Add {
        /// Name of the MCP server
        name: String,
        /// MCP server configuration (JSON string or file path)
        #[arg(value_name = "CONFIG")]
        config: Option<String>,
        /// Write to `<project>/.elph/mcp.json` instead of home
        #[arg(long)]
        project: bool,
    },
    /// Remove an MCP server configuration
    Remove {
        /// Name of the MCP server to remove
        name: String,
        /// Remove from project layer only
        #[arg(long)]
        project: bool,
        /// Also try the other layer if not found in the primary scope
        #[arg(long)]
        all: bool,
    },
    /// Diagnose MCP server configuration and connectivity
    Doctor,
    /// Authenticate with an OAuth-enabled MCP server
    Auth {
        /// Name of the MCP server
        name: String,
        /// OAuth scopes (space-separated). Defaults to server config or empty.
        #[arg(long, value_delimiter = ' ')]
        scopes: Vec<String>,
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
        McpCommands::List { project, home } => handle_list(&paths, *project, *home),
        McpCommands::Add { name, config, project } => {
            let Some(raw) = config else {
                help::unimplemented("MCP add — interactive config entry not yet implemented");
                return EXIT_SUCCESS;
            };
            let scope = if *project {
                McpConfigScope::Project
            } else {
                McpConfigScope::Home
            };
            match mcp_runtime::parse_server_config(raw) {
                Ok(server) => match mcp_runtime::upsert_server_in(&paths, scope, name, server) {
                    Ok(()) => {
                        let _ = mcp_runtime::ensure_project_mcp_cache(&paths);
                        println!(
                            "Saved MCP server '{name}' to {} ({})",
                            mcp_runtime::config_path(&paths, scope).display(),
                            scope.label()
                        );
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
        McpCommands::Remove { name, project, all } => {
            let primary = if *project {
                McpConfigScope::Project
            } else {
                McpConfigScope::Home
            };
            let mut removed_any = false;
            match mcp_runtime::remove_server_in(&paths, primary, name) {
                Ok(true) => {
                    println!(
                        "Removed MCP server '{name}' from {} ({})",
                        mcp_runtime::config_path(&paths, primary).display(),
                        primary.label()
                    );
                    removed_any = true;
                }
                Ok(false) if *all => {}
                Ok(false) => {
                    eprintln!(
                        "MCP server '{name}' not found in {} layer. Try --project or --all.",
                        primary.label()
                    );
                    return EXIT_ERROR;
                }
                Err(error) => {
                    eprintln!("{error}");
                    return EXIT_ERROR;
                }
            }
            if *all {
                let other = match primary {
                    McpConfigScope::Home => McpConfigScope::Project,
                    McpConfigScope::Project => McpConfigScope::Home,
                };
                match mcp_runtime::remove_server_in(&paths, other, name) {
                    Ok(true) => {
                        println!(
                            "Removed MCP server '{name}' from {} ({})",
                            mcp_runtime::config_path(&paths, other).display(),
                            other.label()
                        );
                        removed_any = true;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        eprintln!("{error}");
                        return EXIT_ERROR;
                    }
                }
            }
            if removed_any {
                // Only clear OAuth if the name is gone from the merged view.
                if let Ok(merged) = mcp_runtime::load_config(&paths)
                    && !merged.servers.contains_key(name)
                {
                    let auth_store_path = paths.auth_store_path();
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("tokio runtime");
                    let _ = rt.block_on(clear_credentials(&auth_store_path, name));
                }
                EXIT_SUCCESS
            } else {
                eprintln!("MCP server '{name}' not found.");
                EXIT_ERROR
            }
        }
        McpCommands::Doctor => handle_doctor(&paths),
        McpCommands::Auth { name, scopes } => handle_auth(&paths, name, scopes),
        McpCommands::Logout { name } => {
            let auth_store_path = paths.auth_store_path();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            match rt.block_on(clear_credentials(&auth_store_path, name)) {
                Ok(true) => {
                    println!("Cleared OAuth credentials for MCP server '{name}'.");
                    EXIT_SUCCESS
                }
                Ok(false) => {
                    eprintln!("No OAuth credentials found for MCP server '{name}'.");
                    EXIT_ERROR
                }
                Err(error) => {
                    eprintln!("{error}");
                    EXIT_ERROR
                }
            }
        }
    }
}

fn handle_list(paths: &Paths, project_only: bool, home_only: bool) -> ExitCode {
    if project_only && home_only {
        eprintln!("Use only one of --project or --home.");
        return EXIT_ERROR;
    }

    let (config, sources) = if project_only {
        match mcp_runtime::load_layer(paths, McpConfigScope::Project) {
            Ok(c) => (c, None),
            Err(e) => {
                eprintln!("{e}");
                return EXIT_ERROR;
            }
        }
    } else if home_only {
        match mcp_runtime::load_layer(paths, McpConfigScope::Home) {
            Ok(c) => (c, None),
            Err(e) => {
                eprintln!("{e}");
                return EXIT_ERROR;
            }
        }
    } else {
        match (mcp_runtime::load_config(paths), mcp_runtime::server_sources(paths)) {
            (Ok(c), Ok(s)) => (c, Some(s)),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("{e}");
                return EXIT_ERROR;
            }
        }
    };

    println!("Home:    {}", paths.mcp_config_path().display());
    println!("Project: {}", paths.project_mcp_config_path().display());
    if project_only {
        println!("Layer:   project only");
    } else if home_only {
        println!("Layer:   home only");
    } else {
        println!("Layer:   merged (project overrides home)");
    }

    if config.servers.is_empty() {
        println!("No MCP servers configured.");
        return EXIT_SUCCESS;
    }

    let auth_store_path = paths.auth_store_path();
    for (name, server) in &config.servers {
        let disabled = if server.is_disabled() { " [disabled]" } else { "" };
        let oauth = if has_stored_credentials(&auth_store_path, name) {
            " [oauth:authorized]"
        } else if server.wants_oauth() {
            " [oauth:needed]"
        } else {
            ""
        };
        let source = sources
            .as_ref()
            .and_then(|m| m.get(name))
            .map(|s| match s {
                McpServerSource::Home => " [home]",
                McpServerSource::Project => " [project]",
                McpServerSource::ProjectOverHome => " [project>home]",
            })
            .unwrap_or("");
        println!("{name}: type={}{disabled}{oauth}{source}", server.kind_label());
        if let Some(url) = server.remote_url() {
            println!("  url: {url}");
        }
        if let McpServerConfig::Stdio(c) = server {
            println!("  command: {} {:?}", c.command, c.args);
        }
    }
    EXIT_SUCCESS
}

fn handle_auth(paths: &Paths, name: &str, scopes: &[String]) -> ExitCode {
    let config = match mcp_runtime::load_config(paths) {
        Ok(c) => c,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    let Some(server) = config.servers.get(name) else {
        eprintln!("MCP server '{name}' not found. Add it first with `elph mcp add`.");
        return EXIT_ERROR;
    };
    let Some(url) = server.remote_url() else {
        eprintln!("MCP server '{name}' is stdio; OAuth applies only to http/sse servers.");
        return EXIT_ERROR;
    };

    let mut options = server
        .oauth_meta()
        .map(|meta| McpOAuthFlowOptions::from_server_meta(&meta))
        .unwrap_or_default();
    options = options.with_scopes_override(scopes.iter().cloned());
    options.open_browser = true;

    let auth_store_path = paths.auth_store_path();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    match rt.block_on(run_oauth_flow(name, url, &auth_store_path, options)) {
        Ok(result) => {
            println!(
                "OAuth complete for '{name}' (client_id={}). Stored at {}.",
                result.client_id,
                result.credentials_path.display()
            );
            EXIT_SUCCESS
        }
        Err(error) => {
            eprintln!("OAuth failed: {error}");
            EXIT_ERROR
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

    let home = match mcp_runtime::load_layer(paths, McpConfigScope::Home) {
        Ok(c) => c,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    let project = match mcp_runtime::load_layer(paths, McpConfigScope::Project) {
        Ok(c) => c,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    let config = home.merge_with(&project);
    let sources = mcp_runtime::server_sources(paths).unwrap_or_default();

    println!("── MCP doctor ──");
    println!("Home:    {}", paths.mcp_config_path().display());
    println!(
        "         {} server(s){}",
        home.server_count(),
        if paths.mcp_config_path().exists() {
            ""
        } else {
            " (missing)"
        }
    );
    println!("Project: {}", paths.project_mcp_config_path().display());
    println!(
        "         {} server(s){}",
        project.server_count(),
        if paths.project_mcp_config_path().exists() {
            ""
        } else {
            " (missing)"
        }
    );
    println!(
        "Merged:  {} server(s) enabled={} policy_rules={}",
        config.server_count(),
        config.enabled_count(),
        config.policy.allow.len() + config.policy.deny.len() + config.policy.require_approval.len()
    );

    if config.servers.is_empty() {
        println!("No MCP servers configured.");
        return EXIT_SUCCESS;
    }

    let auth_store_path = paths.auth_store_path();
    println!("OAuth store: {} (never printed)", auth_store_path.display());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut ok = true;
    println!();
    println!("── Probe (merged) ──");
    for (name, server) in &config.servers {
        let source = sources
            .get(name)
            .map(|s| match s {
                McpServerSource::Home => "home",
                McpServerSource::Project => "project",
                McpServerSource::ProjectOverHome => "project>home",
            })
            .unwrap_or("?");
        if server.is_disabled() {
            println!("{name}: skipped [disabled] [{source}]");
            continue;
        }
        let auth_report = server
            .http_config()
            .map(|c| McpAuthSourceReport::probe(c, name, Some(&auth_store_path)));
        if let Some(rep) = &auth_report
            && rep.conflict
            && matches!(rep.policy, elph_agent::McpAuthConflictPolicy::Error)
        {
            println!(
                "{name}: conflict [auth] [{source}] — {} (set authConflict preferEnv|preferOauth)",
                rep.status_label()
            );
            ok = false;
            continue;
        }
        let result = rt.block_on(probe_server_with_auth(name, server, Some(&auth_store_path)));
        let status = if result.ok { "ok" } else { "fail" };
        let auth = auth_report
            .as_ref()
            .map(|r| format!(" auth={}", r.status_label()))
            .unwrap_or_default();
        // Never print tokens/headers — only transport + status.
        println!("{name}: {status} [{}] [{source}]{auth} — {}", result.transport, result.message);
        if !result.ok {
            ok = false;
        }
    }

    if let Ok(cache) = mcp_runtime::project_mcp_cache_dir(paths) {
        println!("Project MCP cache: {}", cache.display());
    }

    if ok { EXIT_SUCCESS } else { EXIT_ERROR }
}

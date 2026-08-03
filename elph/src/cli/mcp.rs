use clap::{Parser, Subcommand};

use super::help;
use super::style::{self, CliStyle, S_ACCENT, S_BODY, S_HEADER, S_MUTED, S_OK, S_WARN};
use crate::platform::ensure_home_blocking;
use crate::platform::mcp as mcp_runtime;
use crate::platform::mcp::{McpConfigScope, McpServerSource};
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths, Settings};
use crate::utils::path::AppPaths;
use elph_agent::{McpLifecycleMode, McpOAuthFlowOptions, McpServerConfig};
use elph_agent::{clear_credentials, has_stored_credentials, run_oauth_flow};

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
        #[arg(long)]
        project: bool,
        #[arg(long)]
        home: bool,
    },
    /// Add or update an MCP server configuration
    Add {
        name: String,
        #[arg(value_name = "CONFIG")]
        config: Option<String>,
        #[arg(long)]
        project: bool,
    },
    /// Remove an MCP server configuration
    Remove {
        name: String,
        #[arg(long)]
        project: bool,
        #[arg(long)]
        all: bool,
    },
    /// Diagnose MCP server configuration and connectivity
    Doctor,
    /// Authenticate with an OAuth-enabled MCP server
    Auth {
        name: String,
        #[arg(long, value_delimiter = ' ')]
        scopes: Vec<String>,
    },
    /// Remove OAuth credentials for an MCP server
    Logout { name: String },
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
                        let _ = mcp_runtime::ensure_mcp_cache(&paths, None);
                        let sty = CliStyle::auto();
                        let mut out = String::new();
                        style::success(
                            &mut out,
                            sty,
                            format!(
                                "Saved MCP server '{name}' to {} ({})",
                                mcp_runtime::config_path(&paths, scope).display(),
                                scope.label()
                            ),
                        );
                        print!("{out}");
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
            let sty = CliStyle::auto();
            let primary = if *project {
                McpConfigScope::Project
            } else {
                McpConfigScope::Home
            };
            let mut removed_any = false;
            match mcp_runtime::remove_server_in(&paths, primary, name) {
                Ok(true) => {
                    let mut out = String::new();
                    style::success(
                        &mut out,
                        sty,
                        format!(
                            "Removed MCP server '{name}' from {} ({})",
                            mcp_runtime::config_path(&paths, primary).display(),
                            primary.label()
                        ),
                    );
                    print!("{out}");
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
                        let mut out = String::new();
                        style::success(
                            &mut out,
                            sty,
                            format!(
                                "Removed MCP server '{name}' from {} ({})",
                                mcp_runtime::config_path(&paths, other).display(),
                                other.label()
                            ),
                        );
                        print!("{out}");
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
            let sty = CliStyle::auto();
            let auth_store_path = paths.auth_store_path();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            match rt.block_on(clear_credentials(&auth_store_path, name)) {
                Ok(true) => {
                    let mut out = String::new();
                    style::success(&mut out, sty, format!("Cleared OAuth credentials for MCP server '{name}'."));
                    print!("{out}");
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
    let sty = CliStyle::auto();
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

    let mut out = String::new();
    style::section(&mut out, sty, "MCP servers");
    style::kv(&mut out, sty, "Home config", paths.mcp_config_path().display());
    style::kv(&mut out, sty, "Project config", paths.project_mcp_config_path().display());

    if project_only {
        style::kv(&mut out, sty, "Layer", "project only");
    } else if home_only {
        style::kv(&mut out, sty, "Layer", "home only");
    } else {
        style::kv(&mut out, sty, "Layer", "merged (project overrides home)");
    }

    if config.servers.is_empty() {
        use std::fmt::Write;
        let _ = writeln!(out);
        style::info(&mut out, sty, sty.paint(S_MUTED, "No MCP servers configured."));
        print!("{out}");
        return EXIT_SUCCESS;
    }

    use std::fmt::Write;
    let _ = writeln!(out);

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

        let _ = writeln!(
            out,
            "  {}  {}{}{}{}",
            sty.paint(S_ACCENT, name),
            sty.paint(S_MUTED, server.kind_label()),
            sty.paint(S_MUTED, disabled),
            sty.paint(S_HEADER, oauth),
            sty.paint(S_MUTED, source),
        );
        let _ = writeln!(
            out,
            "   {}  lifecycle: {}",
            sty.paint(S_MUTED, "·"),
            sty.paint(
                S_BODY,
                match server.lifecycle_mode() {
                    McpLifecycleMode::Auto => "auto",
                    McpLifecycleMode::Legacy => "legacy",
                    McpLifecycleMode::Discover => "discover",
                }
            ),
        );
        if let Some(url) = server.remote_url() {
            let _ = writeln!(out, "   {}  url: {}", sty.paint(S_MUTED, "·"), sty.paint(S_BODY, url));
        }
        if let McpServerConfig::Stdio(c) = server {
            let _ = writeln!(
                out,
                "   {}  command: {} {:?}",
                sty.paint(S_MUTED, "·"),
                sty.paint(S_BODY, &c.command),
                c.args,
            );
        }
    }

    print!("{out}");
    EXIT_SUCCESS
}

fn handle_auth(paths: &Paths, name: &str, scopes: &[String]) -> ExitCode {
    let sty = CliStyle::auto();
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
            let mut out = String::new();
            style::success(
                &mut out,
                sty,
                format!(
                    "OAuth complete for '{name}' (client_id={}). Stored at {}.",
                    result.client_id,
                    result.credentials_path.display()
                ),
            );
            print!("{out}");
            EXIT_SUCCESS
        }
        Err(error) => {
            eprintln!("OAuth failed: {error}");
            EXIT_ERROR
        }
    }
}

fn handle_doctor(paths: &Paths) -> ExitCode {
    let sty = CliStyle::auto();
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

    let mut out = String::new();
    style::section(&mut out, sty, "MCP Doctor");
    style::kv(&mut out, sty, "Home config", paths.mcp_config_path().display());
    style::kv(
        &mut out,
        sty,
        "Home servers",
        format!(
            "{} server(s){}",
            home.server_count(),
            if paths.mcp_config_path().exists() {
                ""
            } else {
                " (file missing)"
            }
        ),
    );
    style::kv(&mut out, sty, "Project config", paths.project_mcp_config_path().display());
    style::kv(
        &mut out,
        sty,
        "Project servers",
        format!(
            "{} server(s){}",
            project.server_count(),
            if paths.project_mcp_config_path().exists() {
                ""
            } else {
                " (file missing)"
            }
        ),
    );
    style::kv(&mut out, sty, "Merged servers", config.server_count());

    if config.servers.is_empty() {
        use std::fmt::Write;
        let _ = writeln!(out);
        style::info(&mut out, sty, sty.paint(S_MUTED, "No MCP servers configured."));
    } else {
        use std::fmt::Write;
        let _ = writeln!(out);
        for (name, server) in &config.servers {
            let source = sources
                .get(name)
                .map(|s| match s {
                    McpServerSource::Home => " [home]",
                    McpServerSource::Project => " [project]",
                    McpServerSource::ProjectOverHome => " [project>home]",
                })
                .unwrap_or("");
            let _ = writeln!(
                out,
                "  {}  {}{}",
                sty.paint(S_ACCENT, name),
                sty.paint(S_MUTED, server.kind_label()),
                sty.paint(S_MUTED, source),
            );
        }
    }

    print!("{out}");
    EXIT_SUCCESS
}

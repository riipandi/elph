use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use super::help;
use super::style::{self, CliStyle, S_ACCENT, S_BODY, S_HEADER, S_MUTED};
use crate::platform::ensure_home_blocking;
use crate::platform::mcp as mcp_runtime;
use crate::platform::mcp::{McpConfigScope, McpServerSource};
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};
use crate::utils::path::AppPaths;
use elph_agent::mcp::{McpLifecycleMode, McpLoadOptions, McpLoadStrategy, McpOAuthFlowOptions, McpServerConfig};
use elph_agent::mcp::{clear_credentials, has_stored_credentials, run_oauth_flow};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum McpTransport {
    /// Launch a local process and communicate over stdin/stdout.
    Stdio,
    /// Connect to a remote server over streamable HTTP.
    Http,
    /// Connect to a remote server over legacy Server-Sent Events.
    Sse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum McpScope {
    /// `CONFIG_DIR/mcp.json`, available in all projects.
    User,
    /// `PROJECT_DIR/.elph/mcp.json`, only for the current project.
    Project,
}

impl McpScope {
    fn config_scope(self) -> McpConfigScope {
        match self {
            Self::User => McpConfigScope::Home,
            Self::Project => McpConfigScope::Project,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

#[derive(Subcommand)]
pub enum McpCommands {
    /// List configured MCP servers (merged home + project).
    List {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
        #[arg(long)]
        project: bool,
        #[arg(long)]
        home: bool,
    },
    /// Add or update an MCP server.
    Add(AddArgs),
    /// Remove an MCP server configuration.
    Remove {
        /// Server name to remove.
        name: String,
        /// Config to remove from. When omitted, all scopes are searched.
        #[arg(short = 's', long, value_enum)]
        scope: Option<McpScope>,
        /// Legacy alias for `--scope project`.
        #[arg(long, hide = true, conflicts_with = "scope")]
        project: bool,
        /// Remove the server from both home and project configs.
        #[arg(long)]
        all: bool,
    },
    /// Enable an MCP server.
    Enable {
        name: String,
        #[arg(short = 's', long, value_enum)]
        scope: Option<McpScope>,
    },
    /// Disable an MCP server.
    Disable {
        name: String,
        #[arg(short = 's', long, value_enum)]
        scope: Option<McpScope>,
    },
    /// Diagnose MCP server configuration and connectivity.
    Doctor {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
        /// Check only this server.
        name: Option<String>,
    },
    /// Authenticate with an OAuth-enabled MCP server
    Auth {
        name: String,
        #[arg(long, value_delimiter = ' ')]
        scopes: Vec<String>,
    },
    /// Remove OAuth credentials for an MCP server
    Logout { name: String },
}

const ADD_AFTER_HELP: &str = "\
Examples:
  # Add a stdio server (everything after -- is the server command)
  elph mcp add filesystem -- npx -y @modelcontextprotocol/server-filesystem /tmp

  # Add a stdio server with environment variables
  elph mcp add postgres -e DATABASE_URL=postgres://localhost/mydb -- npx -y server-postgres

  # Add a remote HTTP server
  elph mcp add --transport http sentry https://mcp.sentry.dev/mcp

  # Add a remote server with an authentication header
  elph mcp add --transport http api https://mcp.example.com/mcp --header 'Authorization: Bearer TOKEN'

  # Add to the project config instead of the user config
  elph mcp add --scope project github -- npx -y @modelcontextprotocol/server-github";

/// Arguments accepted by `mcp add`.
#[derive(Debug, Clone, Args)]
#[command(after_help = ADD_AFTER_HELP)]
pub struct AddArgs {
    /// Server name. Only letters, numbers, hyphens, and underscores are allowed.
    pub name: String,
    /// Command to launch (stdio), or URL to connect to (http/sse).
    #[arg(value_name = "COMMAND_OR_URL", group = "source")]
    command_or_url: Option<String>,
    /// Arguments passed to the server command. Put them after `--`.
    #[arg(value_name = "ARGS")]
    args: Vec<String>,
    /// Transport. Defaults to stdio, or infers HTTP for a bare http(s) URL.
    #[arg(short = 't', long, value_enum)]
    transport: Option<McpTransport>,
    /// Config to write to: user or project.
    #[arg(short = 's', long, value_enum, default_value = "user")]
    scope: McpScope,
    /// Environment variable for the server process (repeatable).
    #[arg(short = 'e', long = "env", value_name = "KEY=value")]
    env: Vec<String>,
    /// HTTP header for remote servers (repeatable).
    #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
    header: Vec<String>,
    /// Read one server object from JSON (legacy form).
    #[arg(long, hide = true, conflicts_with = "source")]
    config: Option<String>,
    /// Legacy alias for `--scope project`.
    #[arg(long, hide = true, conflicts_with = "scope")]
    project: bool,
}

pub fn handle(args: &McpArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<McpArgs>();
    };

    let paths = match ensure_home_blocking(env!("CARGO_PKG_VERSION")) {
        Ok(paths) => paths,
        Err(error) => {
            log::error!("MCP home bootstrap failed: {error:#}");
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };

    match cmd {
        McpCommands::List { json, project, home } => handle_list(&paths, *project, *home, *json),
        McpCommands::Add(args) => handle_add(&paths, args),
        McpCommands::Remove {
            name,
            scope,
            project,
            all,
        } => handle_remove(&paths, name, *scope, *project, *all),
        McpCommands::Enable { name, scope } => handle_set_enabled(&paths, name, *scope, true),
        McpCommands::Disable { name, scope } => handle_set_enabled(&paths, name, *scope, false),
        McpCommands::Doctor { json, name } => handle_doctor(&paths, *json, name.as_deref()),
        McpCommands::Auth { name, scopes } => handle_auth(&paths, name, scopes),
        McpCommands::Logout { name } => {
            let sty = CliStyle::auto();
            let auth_store_path = paths.auth_store_path();
            match elph_agent::runtime::try_block_on(clear_credentials(&auth_store_path, name)).and_then(|result| result)
            {
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

#[derive(Debug)]
struct ResolvedAdd {
    server: McpServerConfig,
    warnings: Vec<String>,
}

fn handle_add(paths: &Paths, args: &AddArgs) -> ExitCode {
    let scope = if args.project {
        McpConfigScope::Project
    } else {
        args.scope.config_scope()
    };
    let resolved = match resolve_add(args) {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    for warning in &resolved.warnings {
        eprintln!("{warning}");
    }

    let name = &args.name;
    if let Err(error) = mcp_runtime::upsert_server_in(paths, scope, name, resolved.server.clone()) {
        eprintln!("{error}");
        return EXIT_ERROR;
    }
    // The cache directory is useful to the runtime even when this command is
    // run before the first interactive session. Failure to create it should
    // not make a successfully persisted config look like a failed add.
    let _ = mcp_runtime::ensure_mcp_cache(paths, None);

    let description = match &resolved.server {
        McpServerConfig::Stdio(config) => {
            let command = std::iter::once(config.command.as_str())
                .chain(config.args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            format!("stdio MCP server '{name}' with command: {command}")
        }
        McpServerConfig::Http(config) => format!("HTTP MCP server '{name}' with URL: {}", config.url),
        McpServerConfig::Sse(config) => format!("SSE MCP server '{name}' with URL: {}", config.url),
    };
    let mut out = String::new();
    style::success(
        &mut out,
        CliStyle::auto(),
        format!(
            "Added {description} to {} config\nFile modified: {}",
            if args.project { "project" } else { args.scope.label() },
            mcp_runtime::config_path(paths, scope).display()
        ),
    );
    print!("{out}");
    EXIT_SUCCESS
}

fn resolve_add(args: &AddArgs) -> Result<ResolvedAdd> {
    validate_server_name(&args.name)?;

    if let Some(raw) = args.config.as_deref() {
        if args.command_or_url.is_some()
            || !args.args.is_empty()
            || args.transport.is_some()
            || !args.env.is_empty()
            || !args.header.is_empty()
        {
            bail!("--config cannot be combined with a command, URL, transport, environment, or header");
        }
        return Ok(ResolvedAdd {
            server: mcp_runtime::parse_server_config(raw)?,
            warnings: Vec::new(),
        });
    }

    // Preserve the old `elph mcp add NAME '{\"type\": ...}'` and
    // `elph mcp add NAME path/to/server.json` forms while making a command
    // the normal positional interface.
    if let Some(source) = args.command_or_url.as_deref()
        && (source.trim_start().starts_with('{') || Path::new(source).is_file())
        && args.transport.is_none()
        && args.args.is_empty()
        && args.env.is_empty()
        && args.header.is_empty()
    {
        return Ok(ResolvedAdd {
            server: mcp_runtime::parse_server_config(source)?,
            warnings: Vec::new(),
        });
    }

    let inferred_http = args.transport.is_none()
        && args.args.is_empty()
        && args.env.is_empty()
        && args.command_or_url.as_deref().is_some_and(is_http_url);
    let transport = args.transport.unwrap_or(if inferred_http {
        McpTransport::Http
    } else {
        McpTransport::Stdio
    });
    let source = args.command_or_url.as_deref();

    match transport {
        McpTransport::Stdio => {
            let Some(command) = source else {
                bail!("A command is required for stdio servers. Usage: elph mcp add <name> -- <command> [args...]");
            };
            if !args.header.is_empty() {
                bail!("--header can only be used with HTTP or SSE servers");
            }
            if looks_like_env_pair(command) {
                bail!(
                    "Invalid command '{command}': it looks like an environment variable. \
                     Pass each variable as its own flag: {}",
                    args.env
                        .iter()
                        .chain(std::iter::once(&command.to_string()))
                        .map(|pair| format!("-e {pair}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
            let env = parse_env_vars(&args.env)?;
            let mut warnings = Vec::new();
            if args.transport.is_none() && looks_like_url(command) {
                let suggested_url = if is_http_url(command) {
                    command.to_string()
                } else {
                    format!("http://{command}")
                };
                warnings.push(format!(
                    "Warning: '{command}' looks like a URL, but it is being added as a stdio command. \
                     For a remote server, use: elph mcp add --transport http {} {suggested_url}",
                    args.name,
                ));
            }
            let mut server = McpServerConfig::stdio(command, args.args.clone());
            if let McpServerConfig::Stdio(config) = &mut server {
                config.env = env;
            }
            Ok(ResolvedAdd { server, warnings })
        }
        McpTransport::Http | McpTransport::Sse => {
            let Some(url) = source else {
                bail!(
                    "A URL is required for {} servers. Usage: elph mcp add --transport {} <name> <url>",
                    if transport == McpTransport::Sse { "SSE" } else { "HTTP" },
                    if transport == McpTransport::Sse { "sse" } else { "http" }
                );
            };
            if !is_http_url(url) {
                bail!("Invalid URL '{url}'. Server URLs must start with http:// or https://.");
            }
            if !args.args.is_empty() {
                bail!(
                    "Unexpected arguments after the URL: '{}'. HTTP and SSE servers take a single URL.",
                    args.args.join(" ")
                );
            }
            if !args.env.is_empty() {
                bail!("--env can only be used with stdio servers");
            }
            let headers = parse_headers(&args.header)?;
            let mut server = if transport == McpTransport::Sse {
                McpServerConfig::sse(url)
            } else {
                McpServerConfig::http(url)
            };
            match &mut server {
                McpServerConfig::Http(config) | McpServerConfig::Sse(config) => {
                    config.headers = headers;
                }
                McpServerConfig::Stdio(_) => unreachable!("transport selected a remote config"),
            }
            let mut warnings = Vec::new();
            if inferred_http {
                warnings.push(format!(
                    "No --transport given; '{url}' starts with http(s)://, adding as an HTTP server. \
                     Use --transport sse for an SSE server, or --transport stdio to force a stdio command."
                ));
            }
            Ok(ResolvedAdd { server, warnings })
        }
    }
}

fn validate_server_name(name: &str) -> Result<()> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("Invalid name '{name}'. Names can only contain letters, numbers, hyphens, and underscores.");
    }
    Ok(())
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn looks_like_url(value: &str) -> bool {
    is_http_url(value) || value.starts_with("localhost")
}

fn looks_like_env_pair(value: &str) -> bool {
    let Some((key, _)) = value.split_once('=') else {
        return false;
    };
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_env_vars(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut parsed = BTreeMap::new();
    for value in values {
        let Some((key, value)) = value.split_once('=') else {
            bail!(
                "Invalid environment variable format: '{value}'. \
                 Use: -e KEY=value"
            );
        };
        let mut key_chars = key.chars();
        if !key_chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !key_chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            bail!("Invalid environment variable name '{key}' in '-e {value}'");
        }
        parsed.insert(key.to_string(), value.to_string());
    }
    Ok(parsed)
}

fn parse_headers(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut parsed = BTreeMap::new();
    for value in values {
        let Some((name, header_value)) = value.split_once(':') else {
            bail!("Invalid header format: '{value}'. Expected format: 'Name: value'");
        };
        let name = name.trim();
        if name.is_empty() {
            bail!("Invalid header: '{value}'. Header name cannot be empty.");
        }
        parsed.insert(name.to_string(), header_value.trim().to_string());
    }
    Ok(parsed)
}

fn handle_list(paths: &Paths, project_only: bool, home_only: bool, json: bool) -> ExitCode {
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

    if json {
        let auth_store_path = paths.auth_store_path();
        let entries: Vec<serde_json::Value> = config
            .servers
            .iter()
            .map(|(name, server)| {
                let source = sources.as_ref().and_then(|map| map.get(name));
                let scope = match source {
                    Some(McpServerSource::Home) => "home",
                    Some(McpServerSource::Project) => "project",
                    Some(McpServerSource::ProjectOverHome) => "project",
                    None if project_only => "project",
                    None => "home",
                };
                let mut entry = serde_json::to_value(server).unwrap_or_default();
                if let Some(object) = entry.as_object_mut() {
                    object.insert("name".into(), serde_json::Value::String(name.clone()));
                    object.insert("scope".into(), serde_json::Value::String(scope.into()));
                    object.insert("enabled".into(), serde_json::Value::Bool(!server.is_disabled()));
                    object.insert(
                        "oauthAuthorized".into(),
                        serde_json::Value::Bool(has_stored_credentials(&auth_store_path, name)),
                    );
                }
                entry
            })
            .collect();
        match serde_json::to_string_pretty(&entries) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("Could not render MCP server list as JSON: {error}");
                return EXIT_ERROR;
            }
        }
        return EXIT_SUCCESS;
    }

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

fn handle_remove(
    paths: &Paths,
    name: &str,
    requested_scope: Option<McpScope>,
    legacy_project: bool,
    all: bool,
) -> ExitCode {
    let requested_scope = if legacy_project {
        Some(McpScope::Project)
    } else {
        requested_scope
    };
    let sites = match remove_sites(paths, name, requested_scope, all) {
        Ok(sites) => sites,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    if sites.is_empty() {
        eprintln!("No MCP server named '{name}' in the requested config.");
        return EXIT_ERROR;
    }

    let mut removed = false;
    for (scope, _) in sites {
        match mcp_runtime::remove_server_in(paths, scope, name) {
            Ok(true) => {
                println!("Removed MCP server '{name}' from {} config", scope.label());
                println!("File modified: {}", mcp_runtime::config_path(paths, scope).display());
                removed = true;
            }
            Ok(false) => {}
            Err(error) => {
                eprintln!("{error}");
                return EXIT_ERROR;
            }
        }
    }
    if !removed {
        eprintln!("No MCP server named '{name}' in the requested config.");
        return EXIT_ERROR;
    }

    // OAuth credentials belong to the server name, not to a config layer.
    // Keep them while an overridden definition remains.
    if let Ok(config) = mcp_runtime::load_config(paths)
        && !config.servers.contains_key(name)
    {
        let auth_store_path = paths.auth_store_path();
        if let Err(error) =
            elph_agent::runtime::try_block_on(clear_credentials(&auth_store_path, name)).and_then(|result| result)
        {
            eprintln!("Could not clear OAuth credentials: {error}");
            return EXIT_ERROR;
        }
    }
    EXIT_SUCCESS
}

fn remove_sites(
    paths: &Paths,
    name: &str,
    requested_scope: Option<McpScope>,
    all: bool,
) -> Result<Vec<(McpConfigScope, PathBuf)>> {
    let home = mcp_runtime::load_layer(paths, McpConfigScope::Home)?;
    let project = mcp_runtime::load_layer(paths, McpConfigScope::Project)?;
    let home_defined = home.servers.contains_key(name);
    let project_defined = project.servers.contains_key(name);

    if all {
        return Ok([
            (McpConfigScope::Home, home_defined),
            (McpConfigScope::Project, project_defined),
        ]
        .into_iter()
        .filter(|(_, defined)| *defined)
        .map(|(scope, _)| (scope, mcp_runtime::config_path(paths, scope)))
        .collect());
    }

    match requested_scope {
        Some(scope) => {
            let defined = match scope {
                McpScope::User => home_defined,
                McpScope::Project => project_defined,
            };
            Ok(defined
                .then(|| {
                    let scope = scope.config_scope();
                    (scope, mcp_runtime::config_path(paths, scope))
                })
                .into_iter()
                .collect())
        }
        None => match (home_defined, project_defined) {
            (true, true) => bail!(
                "MCP server '{name}' exists in both user and project configs. \
                 Specify which one to remove with --scope user or --scope project."
            ),
            (true, false) => Ok(vec![(
                McpConfigScope::Home,
                mcp_runtime::config_path(paths, McpConfigScope::Home),
            )]),
            (false, true) => Ok(vec![(
                McpConfigScope::Project,
                mcp_runtime::config_path(paths, McpConfigScope::Project),
            )]),
            (false, false) => Ok(Vec::new()),
        },
    }
}

fn handle_set_enabled(paths: &Paths, name: &str, requested_scope: Option<McpScope>, enabled: bool) -> ExitCode {
    let home = match mcp_runtime::load_layer(paths, McpConfigScope::Home) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    let project = match mcp_runtime::load_layer(paths, McpConfigScope::Project) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    let scope = requested_scope.map(McpScope::config_scope).or_else(|| {
        project
            .servers
            .contains_key(name)
            .then_some(McpConfigScope::Project)
            .or_else(|| home.servers.contains_key(name).then_some(McpConfigScope::Home))
    });
    let Some(scope) = scope else {
        eprintln!("No MCP server named '{name}'.");
        return EXIT_ERROR;
    };

    let mut config = match mcp_runtime::load_layer(paths, scope) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };
    let Some(server) = config.servers.get_mut(name) else {
        eprintln!("No MCP server named '{name}' in {} config.", scope.label());
        return EXIT_ERROR;
    };
    let was_enabled = !server.is_disabled();
    if was_enabled == enabled {
        println!(
            "MCP server '{name}' is already {}.",
            if enabled { "enabled" } else { "disabled" }
        );
        return EXIT_SUCCESS;
    }
    match server {
        McpServerConfig::Stdio(server) => server.enable = enabled,
        McpServerConfig::Http(server) => server.enable = enabled,
        McpServerConfig::Sse(server) => server.enable = enabled,
    }
    if let Err(error) = mcp_runtime::save_layer(paths, scope, &config) {
        eprintln!("{error}");
        return EXIT_ERROR;
    }
    println!(
        "{} MCP server '{name}' in {} config",
        if enabled { "Enabled" } else { "Disabled" },
        scope.label()
    );
    println!("File modified: {}", mcp_runtime::config_path(paths, scope).display());
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
    match elph_agent::runtime::try_block_on(run_oauth_flow(name, url, &auth_store_path, options))
        .and_then(|result| result)
    {
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

#[derive(Debug, Serialize)]
struct DoctorServer {
    name: String,
    transport: String,
    target: String,
    scope: String,
    status: String,
    healthy: bool,
    tool_count: usize,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    config_sources: Vec<DoctorConfigSource>,
    servers: Vec<DoctorServer>,
    healthy_count: usize,
    failing_count: usize,
}

#[derive(Debug, Serialize)]
struct DoctorConfigSource {
    path: String,
    exists: bool,
    server_count: usize,
    error: Option<String>,
}

fn handle_doctor(paths: &Paths, json: bool, name: Option<&str>) -> ExitCode {
    let (home, home_error) = match mcp_runtime::load_layer(paths, McpConfigScope::Home) {
        Ok(config) => (config, None),
        Err(error) => (elph_agent::mcp::McpConfig::default(), Some(error.to_string())),
    };
    let (project, project_error) = match mcp_runtime::load_layer(paths, McpConfigScope::Project) {
        Ok(config) => (config, None),
        Err(error) => (elph_agent::mcp::McpConfig::default(), Some(error.to_string())),
    };
    let config_error = home_error.is_some() || project_error.is_some();
    let config = home.merge_with(&project);
    let sources = mcp_runtime::server_sources(paths).unwrap_or_default();
    let selected: Vec<(&String, &McpServerConfig)> = config
        .servers
        .iter()
        .filter(|(server_name, _)| name.is_none_or(|wanted| wanted == server_name.as_str()))
        .collect();
    if let Some(wanted) = name
        && selected.is_empty()
    {
        eprintln!("MCP server '{wanted}' not found.");
        if !config.servers.is_empty() {
            eprintln!(
                "Available servers: {}",
                config.servers.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        }
        return EXIT_ERROR;
    }

    let mut reports = Vec::new();
    let mut enabled_config = config.clone();
    enabled_config.servers.retain(|server_name, server| {
        selected.iter().any(|(selected_name, _)| *selected_name == server_name) && !server.is_disabled()
    });
    let probe_reports = if enabled_config.servers.is_empty() {
        Vec::new()
    } else {
        let auth_store_path = paths.auth_store_path();
        match elph_agent::runtime::try_block_on(elph_agent::mcp::McpToolRegistry::load_with_options(
            enabled_config,
            McpLoadOptions {
                // Doctor must contact servers even when normal startup is lazy.
                load_strategy: McpLoadStrategy::Eager,
                discovery_timeout: Some(Duration::from_secs(10)),
                auth_store_path: Some(auth_store_path),
                discover_resources_and_prompts: false,
                enable_list_changed: false,
                ..McpLoadOptions::default()
            },
        ))
        .and_then(|result| result)
        {
            Ok(registry) => registry.load_report().servers,
            Err(error) => {
                eprintln!("MCP doctor probe failed: {error}");
                return EXIT_ERROR;
            }
        }
    };
    for (server_name, server) in selected {
        let source = sources.get(server_name);
        let scope = match source {
            Some(McpServerSource::Home) => "home",
            Some(McpServerSource::Project) => "project",
            Some(McpServerSource::ProjectOverHome) => "project (over home)",
            None => "unknown",
        };
        let (status, healthy, tool_count, detail) = if server.is_disabled() {
            ("disabled".to_string(), true, 0, None)
        } else if let Some(report) = probe_reports.iter().find(|report| report.name == *server_name) {
            (
                if report.ok { "ok" } else { "failed" }.to_string(),
                report.ok,
                report.tool_count,
                Some(report.message.clone()),
            )
        } else {
            ("failed".to_string(), false, 0, Some("server was not probed".to_string()))
        };
        let target = server
            .remote_url()
            .map(str::to_owned)
            .or_else(|| match server {
                McpServerConfig::Stdio(config) => Some(
                    std::iter::once(config.command.as_str())
                        .chain(config.args.iter().map(String::as_str))
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                McpServerConfig::Http(_) | McpServerConfig::Sse(_) => None,
            })
            .unwrap_or_default();
        reports.push(DoctorServer {
            name: server_name.clone(),
            transport: server.kind_label().to_string(),
            target,
            scope: scope.to_string(),
            status,
            healthy,
            tool_count,
            detail,
        });
    }
    let healthy_count = reports.iter().filter(|report| report.healthy).count();
    let failing_count = reports.len().saturating_sub(healthy_count);
    let report = DoctorReport {
        config_sources: vec![
            DoctorConfigSource {
                path: paths.mcp_config_path().display().to_string(),
                exists: paths.mcp_config_path().exists(),
                server_count: home.server_count(),
                error: home_error,
            },
            DoctorConfigSource {
                path: paths.project_mcp_config_path().display().to_string(),
                exists: paths.project_mcp_config_path().exists(),
                server_count: project.server_count(),
                error: project_error,
            },
        ],
        servers: reports,
        healthy_count,
        failing_count,
    };
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("Could not render MCP doctor report as JSON: {error}");
                return EXIT_ERROR;
            }
        }
    } else {
        let sty = CliStyle::auto();
        let mut out = String::new();
        style::section(&mut out, sty, "MCP Doctor");
        style::kv(&mut out, sty, "Home config", paths.mcp_config_path().display());
        style::kv(&mut out, sty, "Project config", paths.project_mcp_config_path().display());
        style::kv(&mut out, sty, "Servers checked", report.servers.len());
        use std::fmt::Write;
        let _ = writeln!(out);
        for source in &report.config_sources {
            if let Some(error) = &source.error {
                let _ = writeln!(
                    out,
                    "  {} {}: {}",
                    sty.paint(S_HEADER, "config error"),
                    sty.paint(S_MUTED, &source.path),
                    error
                );
            }
        }
        if report.servers.is_empty() {
            style::info(&mut out, sty, sty.paint(S_MUTED, "No MCP servers configured."));
        } else {
            for server in &report.servers {
                let marker = if server.healthy { "ok" } else { "failed" };
                let _ = writeln!(
                    out,
                    "  {}  {}  {} ({})",
                    sty.paint(if server.healthy { S_ACCENT } else { S_HEADER }, marker),
                    sty.paint(S_ACCENT, &server.name),
                    sty.paint(S_MUTED, &server.target),
                    server.status
                );
                if let Some(detail) = &server.detail
                    && !detail.is_empty()
                {
                    let _ = writeln!(out, "       {}", sty.paint(S_BODY, detail));
                }
            }
            let _ = writeln!(out, "\n  {} healthy, {} failing", report.healthy_count, report.failing_count);
        }
        print!("{out}");
    }
    if failing_count > 0 || config_error {
        EXIT_ERROR
    } else {
        EXIT_SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_add(argv: &[&str]) -> AddArgs {
        let args = McpArgs::try_parse_from(argv).expect("arguments should parse");
        match args.command {
            Some(McpCommands::Add(args)) => args,
            _ => panic!("expected mcp add"),
        }
    }

    #[test]
    fn add_resolves_stdio_command_env_and_arguments() {
        let args = parse_add(&[
            "mcp",
            "add",
            "filesystem",
            "-e",
            "ROOT=/tmp",
            "-e",
            "MODE=read=only",
            "--",
            "npx",
            "-y",
            "server-filesystem",
            "/tmp",
        ]);
        let resolved = resolve_add(&args).expect("stdio config");
        match resolved.server {
            McpServerConfig::Stdio(config) => {
                assert_eq!(config.command, "npx");
                assert_eq!(
                    config.args,
                    ["-y", "server-filesystem", "/tmp"]
                        .into_iter()
                        .map(String::from)
                        .collect::<Vec<_>>()
                );
                assert_eq!(config.env.get("ROOT").map(String::as_str), Some("/tmp"));
                assert_eq!(config.env.get("MODE").map(String::as_str), Some("read=only"));
            }
            other => panic!("expected stdio config, got {other:?}"),
        }
    }

    #[test]
    fn add_infers_http_and_parses_headers() {
        let args = parse_add(&[
            "mcp",
            "add",
            "remote",
            "https://example.test/mcp",
            "--header",
            "Authorization: Bearer token",
        ]);
        let resolved = resolve_add(&args).expect("http config");
        match resolved.server {
            McpServerConfig::Http(config) => {
                assert_eq!(config.url, "https://example.test/mcp");
                assert_eq!(config.headers.get("Authorization").map(String::as_str), Some("Bearer token"));
            }
            other => panic!("expected http config, got {other:?}"),
        }
        assert_eq!(resolved.warnings.len(), 1);
    }

    #[test]
    fn add_rejects_transport_specific_options() {
        let args = parse_add(&["mcp", "add", "local", "-H", "X: y", "--", "server"]);
        let error = resolve_add(&args).expect_err("stdio cannot have headers");
        assert!(error.to_string().contains("--header"));

        let args = parse_add(&[
            "mcp",
            "add",
            "--transport",
            "http",
            "remote",
            "https://example.test/mcp",
            "-e",
            "TOKEN=value",
        ]);
        let error = resolve_add(&args).expect_err("http cannot have env");
        assert!(error.to_string().contains("--env"));
    }

    #[test]
    fn add_requires_source_and_valid_name() {
        let args = parse_add(&["mcp", "add", "local"]);
        assert!(
            resolve_add(&args)
                .expect_err("source is required")
                .to_string()
                .contains("command is required")
        );

        let args = parse_add(&["mcp", "add", "bad name", "--", "server"]);
        assert!(
            resolve_add(&args)
                .expect_err("name is invalid")
                .to_string()
                .contains("Invalid name")
        );
    }

    #[test]
    fn add_supports_sse_and_project_scope() {
        let args = parse_add(&[
            "mcp",
            "add",
            "--transport",
            "sse",
            "--scope",
            "project",
            "legacy",
            "http://localhost:3000/sse",
        ]);
        assert_eq!(args.scope, McpScope::Project);
        let resolved = resolve_add(&args).expect("sse config");
        assert!(matches!(resolved.server, McpServerConfig::Sse(_)));
    }

    #[test]
    fn remove_sites_detects_ambiguous_definitions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = Paths::from_dirs(tmp.path().join("config"), tmp.path().join("data"), tmp.path().join("project"));
        mcp_runtime::upsert_server_in(&paths, McpConfigScope::Home, "same", McpServerConfig::stdio("home", Vec::new()))
            .expect("home server");
        mcp_runtime::upsert_server_in(
            &paths,
            McpConfigScope::Project,
            "same",
            McpServerConfig::stdio("project", Vec::new()),
        )
        .expect("project server");
        let error = remove_sites(&paths, "same", None, false).expect_err("ambiguous remove");
        assert!(error.to_string().contains("--scope"));
        assert_eq!(
            remove_sites(&paths, "same", Some(McpScope::Project), false)
                .expect("scoped remove")
                .len(),
            1
        );
    }
}

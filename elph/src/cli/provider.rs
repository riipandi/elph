use std::fmt;
use std::sync::Arc;

use anstyle::{AnsiColor, Color, Style};
use clap::{Parser, Subcommand};
use inquire::Select;

use super::help;
use super::interactive;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};
use crate::tui::provider_connect_dialog::{
    ProviderAuthMethod, ProviderConfigStatus, ProviderOption, get_provider_options,
};
use crate::tui::provider_credential_store::save_provider_credential;
use crate::tui::provider_credential_store::save_provider_env_ref;
use crate::utils::path::AppPaths;

// ── Style helpers ────────────────────────────────────────────────────

const STYLE_OK: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Green)));
const STYLE_ERR: Style = Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red)));
const STYLE_MUTED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack)));
const STYLE_BOLD: Style = Style::new().bold();

fn ok(msg: impl fmt::Display) -> String {
    format!("{}✓ {}{}", STYLE_OK.render(), msg, STYLE_OK.render_reset())
}

fn err(msg: impl fmt::Display) -> String {
    format!("{}! {}{}", STYLE_ERR.render(), msg, STYLE_ERR.render_reset())
}

#[derive(Parser, Default)]
#[command(
    name = "provider",
    about = "Manage AI providers and credentials",
    color = clap::ColorChoice::Auto
)]
pub struct ProviderArgs {
    #[command(subcommand)]
    pub command: Option<ProviderCommands>,
}

#[derive(Subcommand)]
pub enum ProviderCommands {
    /// List configured providers and stored credentials
    #[command(visible_alias = "ls")]
    List {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Sign in to an AI provider (interactive login)
    Connect {
        /// Provider ID to connect (e.g. anthropic, openai-codex, github-copilot)
        provider: Option<String>,
        /// Register an environment variable instead of entering a key. Must be used with --provider.
        #[arg(long, value_name = "VAR")]
        env: Option<String>,
    },
    /// Sign out from an AI provider and clear stored credentials
    Disconnect {
        /// Provider ID to disconnect (disconnects all if omitted)
        provider: Option<String>,
    },
    /// Update provider metadata and credentials
    Update {
        /// Provider ID to update (updates all if omitted)
        provider_id: Option<String>,
    },
}

pub fn handle(args: &ProviderArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<ProviderArgs>();
    };

    match cmd {
        ProviderCommands::List { json } => handle_list(json),
        ProviderCommands::Connect { provider, env } => handle_connect(provider.as_deref(), env.as_deref()),
        ProviderCommands::Disconnect { provider } => handle_disconnect(provider.as_deref()),
        ProviderCommands::Update { provider_id } => handle_update(provider_id.as_deref()),
    }
}

fn resolve_paths() -> Result<Paths, ExitCode> {
    Paths::resolve().map_err(|err| {
        help::cli_error(format!("resolve paths: {err}"));
        EXIT_ERROR
    })
}

/// Config status label for display (single line, no newline).
fn config_status_label(status: &ProviderConfigStatus) -> String {
    match status {
        ProviderConfigStatus::Unconfigured => "unconfigured".to_string(),
        ProviderConfigStatus::ApiKeyConfigured => "API key stored".to_string(),
        ProviderConfigStatus::OAuthConfigured => "OAuth configured".to_string(),
        ProviderConfigStatus::EnvVarConfigured(var) => format!("env: {var}"),
    }
}

/// Resolve a human-readable provider name for CLI messages.
///
/// Priority: provider config label → `format_provider_name` → raw id.
fn provider_display_name(id: &str) -> String {
    if let Some(cfg) = crate::agent::provider::provider_config(id) {
        return cfg.label.to_string();
    }
    crate::tui::provider_connect_dialog::format_provider_name(id)
}

fn handle_list(json: &bool) -> ExitCode {
    let paths = match resolve_paths() {
        Ok(p) => p,
        Err(e) => return e,
    };

    let auth_store_path = paths.auth_store_path();
    let provider_ids = crate::tui::provider_credential_store::list_providers_with_credentials(&auth_store_path);

    if *json {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_ids).unwrap_or_else(|_| "[]".into())
        );
        return EXIT_SUCCESS;
    }

    let all_providers = get_provider_options();
    let configured_count = provider_ids.len();

    // Compute live config status for each provider (get_provider_options caches
    // config_status as Unconfigured — we need to read the actual auth file).
    let mut providers_with_status: Vec<ProviderOption> = all_providers
        .into_iter()
        .map(|mut p| {
            p.config_status =
                crate::tui::provider_connect_dialog::get_provider_config_status_at(&auth_store_path, &p.id);
            p
        })
        .collect();
    providers_with_status.sort_by(|a, b| a.id.cmp(&b.id));

    // Group: configured first, then unconfigured
    let configured: Vec<&ProviderOption> = providers_with_status
        .iter()
        .filter(|p| !matches!(p.config_status, ProviderConfigStatus::Unconfigured))
        .collect();
    let unconfigured: Vec<&ProviderOption> = providers_with_status
        .iter()
        .filter(|p| matches!(p.config_status, ProviderConfigStatus::Unconfigured))
        .collect();

    // Find the longest provider name for alignment
    let max_name_len = providers_with_status
        .iter()
        .map(|p| provider_display_name(&p.id).len())
        .max()
        .unwrap_or(10);

    // ── Configured section ────────────────────────────────────────
    if !configured.is_empty() {
        println!();
        println!(
            "{}Configured ({}):{}",
            STYLE_MUTED.render(),
            configured.len(),
            STYLE_MUTED.render_reset()
        );
        for provider in &configured {
            let status = config_status_label(&provider.config_status);
            let display_name = provider_display_name(&provider.id);
            let padded = format!("{:<width$}", display_name, width = max_name_len);
            println!(
                "  {} {}  {}{}{}",
                format_args!("{}✓{}", STYLE_OK.render(), STYLE_OK.render_reset()),
                format_args!("{}{}{}", STYLE_BOLD.render(), padded, STYLE_BOLD.render_reset()),
                STYLE_MUTED.render(),
                status,
                STYLE_MUTED.render_reset(),
            );
        }
    }

    // ── Unconfigured section ──────────────────────────────────────
    if !unconfigured.is_empty() {
        println!();
        println!(
            "{}Unconfigured ({}):{}",
            STYLE_MUTED.render(),
            unconfigured.len(),
            STYLE_MUTED.render_reset()
        );
        for provider in &unconfigured {
            let display_name = provider_display_name(&provider.id);
            let padded = format!("{:<width$}", display_name, width = max_name_len);
            println!("    {}", padded);
        }
    }

    println!();
    if configured_count > 0 {
        println!(
            "{}{}{}{} provider(s) with stored credentials{}",
            STYLE_MUTED.render(),
            STYLE_BOLD.render(),
            configured_count,
            STYLE_BOLD.render_reset(),
            STYLE_MUTED.render_reset(),
        );
    } else {
        println!(
            "{}No configured providers. Use `elph provider connect` to sign in.{}",
            STYLE_MUTED.render(),
            STYLE_MUTED.render_reset()
        );
    }
    EXIT_SUCCESS
}

// ── Connect wizard ───────────────────────────────────────────────────

fn resolve_provider_by_id<'a>(
    providers: &'a [crate::tui::provider_connect_dialog::ProviderOption],
    pid: &str,
) -> Option<(&'a crate::tui::provider_connect_dialog::ProviderOption, ProviderAuthMethod)> {
    let provider = providers.iter().find(|p| p.id == pid)?;
    let method = if provider.supports_api_key {
        ProviderAuthMethod::ApiKey
    } else if provider.supports_oauth {
        ProviderAuthMethod::Account
    } else {
        ProviderAuthMethod::ApiKey
    };
    Some((provider, method))
}

fn handle_connect(provider: Option<&str>, env_var: Option<&str>) -> ExitCode {
    let paths = match resolve_paths() {
        Ok(p) => p,
        Err(e) => return e,
    };

    let auth_store_path = paths.auth_store_path();
    let all_providers = get_provider_options();

    // ── Environment variable reference ─────────────────────────────────
    if let Some(env_var_name) = env_var {
        let Some(pid) = provider else {
            eprintln!("{}", err("The --env flag requires --provider."));
            return EXIT_ERROR;
        };

        // Validate the env var is actually set
        if std::env::var(env_var_name).is_err() {
            eprintln!("{}", err(format!("Environment variable '{env_var_name}' is not set.")));
            return EXIT_ERROR;
        }

        let Some(_) = all_providers.iter().find(|p| p.id == pid) else {
            eprintln!("{}", err(format!("Unknown provider: {pid}")));
            return EXIT_ERROR;
        };

        let auth_store = auth_store_path.clone();
        let pid_owned = pid.to_string();
        let env_owned = env_var_name.to_string();
        let name = provider_display_name(pid);
        match run_async(move || {
            let rt = new_rt();
            rt.block_on(save_provider_env_ref(&auth_store, &pid_owned, &env_owned))
        }) {
            Ok(()) => {
                println!(
                    "{}",
                    ok(format!("Registered {name} to read credential from env: {env_var_name}."))
                );
                return EXIT_SUCCESS;
            }
            Err(e) => {
                eprintln!("{}", err(format!("Failed to register env ref for '{name}': {e}")));
                return EXIT_ERROR;
            }
        }
    }

    // If a specific provider was given, resolve it directly.
    let (selected_provider, auth_method) = if let Some(pid) = provider {
        let name = provider_display_name(pid);
        match resolve_provider_by_id(&all_providers, pid) {
            Some(result) => result,
            None => {
                eprintln!("{}", err(format!("Unknown provider: {name}")));
                return EXIT_ERROR;
            }
        }
    } else {
        // Step 1: pick auth method
        let Some(auth_method) = interactive::select_auth_method() else {
            println!("Cancelled.");
            return EXIT_SUCCESS;
        };

        // Step 2: pick provider with fuzzy search
        let Some(provider) = interactive::select_provider(&all_providers, auth_method) else {
            println!("Cancelled.");
            return EXIT_SUCCESS;
        };

        (provider, auth_method)
    };

    match auth_method {
        // ── OAuth flow ───────────────────────────────────────────────
        ProviderAuthMethod::Account => {
            if !selected_provider.supports_oauth {
                eprintln!("Provider '{}' does not support OAuth login.", selected_provider.id);
                return EXIT_ERROR;
            }

            let callbacks = Arc::new(interactive::CliAuthCallbacks);
            let provider_id = selected_provider.id.clone();
            let provider_name = selected_provider.name.clone();
            let auth_store = auth_store_path.clone();

            match run_async(move || {
                let rt = new_rt();
                let credential = rt.block_on(elph_ai::oauth_provider_login(&provider_id, callbacks))?;
                if let Ok(json) = serde_json::to_string(&credential) {
                    rt.block_on(save_provider_credential(&auth_store, &provider_id, &json))?;
                }
                Ok(credential)
            }) {
                Ok(_) => {
                    println!("{}", ok(format!("Signed in to {provider_name}.")));
                    EXIT_SUCCESS
                }
                Err(e) => {
                    eprintln!("{}", err(format!("OAuth login failed for {provider_name}: {e}")));
                    EXIT_ERROR
                }
            }
        }

        // ── API key flow ─────────────────────────────────────────────
        ProviderAuthMethod::ApiKey => {
            let already_configured = matches!(
                selected_provider.config_status,
                ProviderConfigStatus::ApiKeyConfigured | ProviderConfigStatus::OAuthConfigured
            );
            if already_configured && !interactive::confirm_overwrite(&selected_provider.name) {
                println!("Cancelled.");
                return EXIT_SUCCESS;
            }

            let Some(api_key) = interactive::prompt_api_key(&selected_provider.name) else {
                println!("Cancelled.");
                return EXIT_SUCCESS;
            };

            let auth_store = auth_store_path.clone();
            let pid = selected_provider.id.clone();
            let name = selected_provider.name.clone();
            let pid_for_closure = pid.clone();

            // Detect env: prefix — store as plaintext reference, not encrypted.
            if let Some(env_var) = api_key.strip_prefix("env:") {
                let env_var = env_var.to_string();
                let env_var_for_closure = env_var.clone();
                match run_async(move || {
                    let rt = new_rt();
                    rt.block_on(save_provider_env_ref(&auth_store, &pid_for_closure, &env_var_for_closure))
                }) {
                    Ok(()) => {
                        println!("{}", ok(format!("Registered {name} to read credential from env: {env_var}.")));
                        EXIT_SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{}", err(format!("Failed to register env ref for {name}: {e}")));
                        EXIT_ERROR
                    }
                }
            } else {
                match run_async(move || {
                    let rt = new_rt();
                    rt.block_on(save_provider_credential(&auth_store, &pid_for_closure, &api_key))
                }) {
                    Ok(()) => {
                        println!("{}", ok(format!("Saved API key for {name}.")));
                        EXIT_SUCCESS
                    }
                    Err(e) => {
                        eprintln!("{}", err(format!("Failed to save API key for {name}: {e}")));
                        EXIT_ERROR
                    }
                }
            }
        }
    }
}

/// Run an async closure on a new thread with its own tokio runtime.
/// This avoids the "Cannot start a runtime from within a runtime" panic
/// when called from `#[tokio::main]`.
fn run_async<F, R>(f: F) -> anyhow::Result<R>
where
    F: FnOnce() -> anyhow::Result<R> + Send + 'static,
    R: Send + 'static,
{
    std::thread::spawn(f)
        .join()
        .map_err(|e| anyhow::anyhow!("thread panicked: {:?}", e))?
}

fn new_rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime")
}

// ── Disconnect ───────────────────────────────────────────────────────

fn handle_disconnect(provider: Option<&str>) -> ExitCode {
    let paths = match resolve_paths() {
        Ok(p) => p,
        Err(e) => return e,
    };

    let auth_store_path = paths.auth_store_path();
    let provider_ids = crate::tui::provider_credential_store::list_providers_with_credentials(&auth_store_path);

    if let Some(pid) = provider {
        let name = provider_display_name(pid);
        if !provider_ids.contains(&pid.to_string()) {
            println!("No stored credentials for {name}.");
            return EXIT_SUCCESS;
        }
        let pid = pid.to_string();
        let name_clone = name.clone();
        let auth_store = auth_store_path.clone();
        let pid_for_closure = pid.clone();
        match run_async(move || {
            let rt = new_rt();
            rt.block_on(crate::tui::provider_credential_store::delete_provider_credential(
                &auth_store,
                &pid_for_closure,
            ))
        }) {
            Ok(true) => {
                println!("{}", ok(format!("Signed out from {name_clone}.")));
                EXIT_SUCCESS
            }
            Ok(false) => {
                println!("No stored credentials for {name}.");
                EXIT_SUCCESS
            }
            Err(e) => {
                eprintln!("{}", err(format!("Failed to disconnect {name}: {e}")));
                EXIT_ERROR
            }
        }
    } else {
        if provider_ids.is_empty() {
            println!("No stored provider credentials to disconnect.");
            return EXIT_SUCCESS;
        }

        // Show interactive selection with human-readable names
        let display_items: Vec<String> = provider_ids.iter().map(|id| provider_display_name(id)).collect();
        let display_refs: Vec<&str> = display_items.iter().map(|s| s.as_str()).collect();
        let selected_idx = Select::new("Select provider to disconnect", display_refs)
            .with_page_size(10)
            .with_help_message("↑↓ navigate · Enter confirm · Esc cancel")
            .prompt_skippable()
            .ok()
            .flatten();

        let Some(name) = selected_idx else {
            println!("Cancelled.");
            return EXIT_SUCCESS;
        };

        // Find the actual provider ID from the selected name
        let Some(pid) = provider_ids
            .iter()
            .find(|id| provider_display_name(id) == name)
            .cloned()
        else {
            println!("Cancelled.");
            return EXIT_SUCCESS;
        };

        let auth_store = auth_store_path.clone();
        let name_for_closure = name;
        let pid_for_closure = pid.clone();
        match run_async(move || {
            let rt = new_rt();
            rt.block_on(crate::tui::provider_credential_store::delete_provider_credential(
                &auth_store,
                &pid_for_closure,
            ))
        }) {
            Ok(true) => {
                println!("{}", ok(format!("Signed out from {name_for_closure}.")));
                EXIT_SUCCESS
            }
            Ok(false) => {
                println!("No stored credentials for {name}.");
                EXIT_SUCCESS
            }
            Err(e) => {
                eprintln!("{}", err(format!("Failed to disconnect {name}: {e}")));
                EXIT_ERROR
            }
        }
    }
}

fn handle_update(provider_id: Option<&str>) -> ExitCode {
    help::unimplemented(&format!(
        "Provider update — not yet implemented (provider_id: {})",
        provider_id.unwrap_or("<all>")
    ));
    EXIT_SUCCESS
}

use std::collections::HashMap;
use std::fmt;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::Arc;

use anstyle::{AnsiColor, Color, Style};
use clap::{Parser, Subcommand};
use inquire::Select;
use serde_json::Value;

use elph_ai::UpdatePolicy;

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

/// Print a dim horizontal rule sized to (at least) the given title width.
fn print_rule(width: usize) {
    let width = width.clamp(20, 64);
    println!("{}{}{}", STYLE_MUTED.render(), "─".repeat(width), STYLE_MUTED.render_reset());
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
    /// Update provider model catalogs from the embedded seed
    Update {
        /// Provider ID to update (updates all builtin providers if omitted)
        provider_id: Option<String>,
        /// Apply to all providers without prompting (merge: keeps custom configuration)
        #[arg(long)]
        yes: bool,
        /// Overwrite existing catalog files with the embedded seed (discards custom config)
        #[arg(long)]
        overwrite: bool,
        /// Show what would change without writing anything
        #[arg(long)]
        dry_run: bool,
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
        ProviderCommands::Update {
            provider_id,
            yes,
            overwrite,
            dry_run,
        } => handle_update(provider_id.as_deref(), *yes, *overwrite, *dry_run),
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

/// Interactive provider login used by `elph acp --setup` (ACP Terminal Auth).
pub fn run_interactive_connect() -> ExitCode {
    handle_connect(None, None)
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
                // Reload provider catalog so it's available immediately.
                if let Some(config_dir) = auth_store_path.parent() {
                    let providers_dir = config_dir.join("providers");
                    let _ = crate::agent::install_providers_dir(&providers_dir);
                }
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
                let credential = rt.block_on(elph_ai::auth::oauth_provider_login(
                    &provider_id,
                    callbacks,
                    &elph_ai::ClientIdentity::new("elph", "ELPH"),
                ))?;
                if let Ok(json) = serde_json::to_string(&credential) {
                    rt.block_on(save_provider_credential(&auth_store, &provider_id, &json))?;
                }
                Ok(credential)
            }) {
                Ok(_) => {
                    // Reload provider catalog so it's available immediately.
                    if let Some(config_dir) = auth_store_path.parent() {
                        let providers_dir = config_dir.join("providers");
                        let _ = crate::agent::install_providers_dir(&providers_dir);
                    }
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
                        // Reload provider catalog so it's available immediately.
                        if let Some(config_dir) = auth_store_path.parent() {
                            let providers_dir = config_dir.join("providers");
                            let _ = crate::agent::install_providers_dir(&providers_dir);
                        }
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
                        // Reload provider catalog so it's available immediately.
                        if let Some(config_dir) = auth_store_path.parent() {
                            let providers_dir = config_dir.join("providers");
                            let _ = crate::agent::install_providers_dir(&providers_dir);
                        }
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

fn handle_update(provider_id: Option<&str>, yes: bool, overwrite: bool, dry_run: bool) -> ExitCode {
    let paths = match resolve_paths() {
        Ok(p) => p,
        Err(e) => return e,
    };
    let dir = paths.providers_dir();

    // Resolve which providers to update.
    let providers: Vec<String> = if let Some(pid) = provider_id {
        if !elph_ai::embedded_provider_ids().contains(&pid) {
            eprintln!("{}", err(format!("Unknown builtin provider: {pid}")));
            return EXIT_ERROR;
        }
        vec![pid.to_string()]
    } else {
        elph_ai::embedded_provider_ids().iter().map(|s| s.to_string()).collect()
    };

    let plan = match elph_ai::plan_provider_update(&dir, &providers) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", err(format!("Plan failed: {e}")));
            return EXIT_ERROR;
        }
    };

    if plan.entries.is_empty() {
        println!("{}", ok("No builtin provider catalogs to update."));
        return EXIT_SUCCESS;
    }

    print_plan(&plan);

    if dry_run {
        println!();
        println!(
            "{}Dry run — nothing written. Re-run without --dry-run to apply.{}",
            STYLE_MUTED.render(),
            STYLE_MUTED.render_reset()
        );
        return EXIT_SUCCESS;
    }

    // Decide the policy for each entry.
    let default_policy = if overwrite {
        UpdatePolicy::Overwrite
    } else {
        UpdatePolicy::Merge
    };

    let resolved: HashMap<String, UpdatePolicy> = if yes || overwrite {
        plan.entries
            .iter()
            .map(|e| (e.provider.clone(), default_policy))
            .collect()
    } else {
        match interactive_resolve_conflicts(&dir, &plan) {
            Ok(map) => map,
            Err(code) => return code,
        }
    };

    let resolve = |e: &elph_ai::ProviderUpdatePlanEntry| -> UpdatePolicy {
        *resolved.get(&e.provider).unwrap_or(&default_policy)
    };

    let report = match elph_ai::apply_provider_update(&dir, &plan, resolve) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", err(format!("Update failed: {e}")));
            return EXIT_ERROR;
        }
    };

    print_report(&report);
    EXIT_SUCCESS
}

/// Print the plan: summary counts, then a list of providers that will change.
fn print_plan(plan: &elph_ai::ProviderUpdatePlan) {
    let new = plan
        .entries
        .iter()
        .filter(|e| matches!(e.status, elph_ai::ProviderUpdateStatus::New))
        .count();
    let conflicts = plan.conflicts().len();
    let up = plan
        .entries
        .iter()
        .filter(|e| matches!(e.status, elph_ai::ProviderUpdateStatus::UpToDate))
        .count();

    let title = "Provider catalog update";
    println!();
    println!("{}{}{}", STYLE_BOLD.render(), title, STYLE_BOLD.render_reset());
    print_rule(title.len());

    let summary: [(&str, usize); 3] = [("Up to date", up), ("New", new), ("With changes", conflicts)];
    let label_w = summary.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    for (label, n) in summary {
        println!(
            "  {:<width$}  {}{}{}",
            label,
            STYLE_BOLD.render(),
            n,
            STYLE_BOLD.render_reset(),
            width = label_w
        );
    }

    if new == 0 && conflicts == 0 {
        println!();
        println!(
            "{}All providers are up to date — nothing to do.{}",
            STYLE_MUTED.render(),
            STYLE_MUTED.render_reset()
        );
        return;
    }

    println!();
    let max_name = plan
        .entries
        .iter()
        .map(|e| provider_display_name(&e.provider).len())
        .max()
        .unwrap_or(0)
        .min(40);

    for e in &plan.entries {
        if matches!(e.status, elph_ai::ProviderUpdateStatus::UpToDate) {
            continue;
        }
        let name = provider_display_name(&e.provider);
        let (tag, style) = match e.status {
            elph_ai::ProviderUpdateStatus::New => ("new", STYLE_OK),
            elph_ai::ProviderUpdateStatus::Conflict => ("conflict", STYLE_ERR),
            elph_ai::ProviderUpdateStatus::UpToDate => unreachable!(),
        };
        let detail = match e.status {
            elph_ai::ProviderUpdateStatus::New => {
                format!("+ {} model(s) in seed, not on disk", e.added.len())
            }
            elph_ai::ProviderUpdateStatus::Conflict => {
                let mut s = format!("~ {} model(s) customized on disk (kept by merge)", e.changed.len());
                if !e.added.is_empty() {
                    s.push_str(&format!(", + {} added from seed", e.added.len()));
                }
                s
            }
            elph_ai::ProviderUpdateStatus::UpToDate => String::new(),
        };
        println!(
            "  {:<width$}  {}{}{} - {}{}{}",
            name,
            style.render(),
            tag,
            style.render_reset(),
            STYLE_MUTED.render(),
            detail,
            STYLE_MUTED.render_reset(),
            width = max_name
        );
    }
}

/// Print the applied-change summary.
fn print_report(report: &elph_ai::ProviderUpdateReport) {
    let title = "Provider catalogs updated";
    println!();
    println!("{}{}{}", STYLE_BOLD.render(), title, STYLE_BOLD.render_reset());
    print_rule(title.len());

    let rows: [(&str, usize, Style); 5] = [
        ("Written", report.written, STYLE_OK),
        ("Merged", report.merged, STYLE_BOLD),
        ("Overwritten", report.overwritten, STYLE_ERR),
        ("Skipped", report.skipped, STYLE_MUTED),
        ("Up to date", report.up_to_date, STYLE_MUTED),
    ];
    let label_w = rows.iter().map(|(l, _, _)| l.len()).max().unwrap_or(0);
    for (label, n, style) in rows {
        if n == 0 {
            continue;
        }
        println!(
            "  {:<width$}  {}{}{}",
            label,
            style.render(),
            n,
            style.render_reset(),
            width = label_w
        );
    }

    println!();
    println!(
        "{}Tip: restart elph to load the updated catalogs.{}",
        STYLE_MUTED.render(),
        STYLE_MUTED.render_reset()
    );
}

/// Present each conflicting provider as an `inquire` selector.
///
/// Options: update (merge, keeping custom config), skip, overwrite, show diff,
/// apply-to-all (update/skip/overwrite), and quit. Conflicts require an interactive
/// terminal; otherwise `--yes` / `--overwrite` must be supplied (pipe-safe).
fn interactive_resolve_conflicts(
    dir: &Path,
    plan: &elph_ai::ProviderUpdatePlan,
) -> Result<HashMap<String, UpdatePolicy>, ExitCode> {
    if plan.conflicts().is_empty() {
        return Ok(HashMap::new());
    }
    if !std::io::stdin().is_terminal() {
        eprintln!(
            "{}",
            err("Conflicts require an interactive terminal or --yes / --overwrite to resolve non-interactively.")
        );
        return Err(EXIT_ERROR);
    }

    const UPDATE: &str = "Update (keep custom config)";
    const SKIP: &str = "Skip this provider";
    const OVERWRITE: &str = "Overwrite with embedded seed";
    const DIFF: &str = "Show diff";
    const UPDATE_ALL: &str = "Update all remaining";
    const SKIP_ALL: &str = "Skip all remaining";
    const OVERWRITE_ALL: &str = "Overwrite all remaining";
    const QUIT: &str = "Quit";

    let options = vec![
        UPDATE.to_string(),
        SKIP.to_string(),
        OVERWRITE.to_string(),
        DIFF.to_string(),
        UPDATE_ALL.to_string(),
        SKIP_ALL.to_string(),
        OVERWRITE_ALL.to_string(),
        QUIT.to_string(),
    ];

    let mut map: HashMap<String, UpdatePolicy> = HashMap::new();
    let mut global: Option<UpdatePolicy> = None;

    for entry in plan.conflicts() {
        if let Some(p) = global {
            map.insert(entry.provider.clone(), p);
            continue;
        }

        let mut message = format!(
            "{}Conflict — {}{}",
            STYLE_BOLD.render(),
            provider_display_name(&entry.provider),
            STYLE_BOLD.render_reset()
        );
        if entry.unparsable {
            message.push_str(&format!(
                "  {} (unparsable on disk; merge leaves it untouched)",
                dir.join(format!("{}.json", entry.provider)).display()
            ));
        }
        if !entry.added.is_empty() {
            message.push_str(&format!("  +{} new in seed", entry.added.len()));
        }
        if !entry.changed.is_empty() {
            message.push_str(&format!("  ~{} customized on disk", entry.changed.len()));
        }

        loop {
            let choice = Select::new(&message, options.clone())
                .with_page_size(options.len())
                .with_help_message("↑↓ navigate · Enter select · Esc quit")
                .prompt_skippable();

            match choice {
                Ok(Some(picked)) if picked.as_str() == DIFF => {
                    print_diff_entry(dir, entry);
                    continue;
                }
                Ok(Some(picked)) if picked.as_str() == UPDATE => {
                    map.insert(entry.provider.clone(), UpdatePolicy::Merge);
                    break;
                }
                Ok(Some(picked)) if picked.as_str() == SKIP => {
                    map.insert(entry.provider.clone(), UpdatePolicy::SkipExisting);
                    break;
                }
                Ok(Some(picked)) if picked.as_str() == OVERWRITE => {
                    map.insert(entry.provider.clone(), UpdatePolicy::Overwrite);
                    break;
                }
                Ok(Some(picked)) if picked.as_str() == UPDATE_ALL => {
                    global = Some(UpdatePolicy::Merge);
                    map.insert(entry.provider.clone(), UpdatePolicy::Merge);
                    break;
                }
                Ok(Some(picked)) if picked.as_str() == SKIP_ALL => {
                    global = Some(UpdatePolicy::SkipExisting);
                    map.insert(entry.provider.clone(), UpdatePolicy::SkipExisting);
                    break;
                }
                Ok(Some(picked)) if picked.as_str() == OVERWRITE_ALL => {
                    global = Some(UpdatePolicy::Overwrite);
                    map.insert(entry.provider.clone(), UpdatePolicy::Overwrite);
                    break;
                }
                // QUIT, Esc (None), or any error → abort.
                Ok(Some(_)) | Ok(None) | Err(_) => {
                    println!("Cancelled.");
                    return Err(EXIT_SUCCESS);
                }
            }
        }
    }
    Ok(map)
}

/// Print a concise, field-level diff (no raw JSON) of what `merge` keeps vs
/// `overwrite` replaces, for a single conflicting provider.
fn print_diff_entry(dir: &Path, entry: &elph_ai::ProviderUpdatePlanEntry) {
    print!("{}", format_diff_entry(dir, entry));
}

/// Build a concise, field-level diff string (no raw JSON dumps).
fn format_diff_entry(dir: &Path, entry: &elph_ai::ProviderUpdatePlanEntry) -> String {
    let name = provider_display_name(&entry.provider);
    let mut out = String::new();
    out.push_str(&format!(
        "\n{}Diff — {}{}\n",
        STYLE_BOLD.render(),
        name,
        STYLE_BOLD.render_reset()
    ));

    let path = dir.join(format!("{}.json", entry.provider));
    let disk: Option<Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|b| serde_json::from_str(&b).ok());
    let seed = elph_ai::embedded_provider_json(&entry.provider).and_then(|s| serde_json::from_str::<Value>(&s).ok());

    if !entry.added.is_empty() {
        out.push_str(&format!(
            "{}  Added (in seed, missing on disk):{}\n",
            STYLE_OK.render(),
            STYLE_OK.render_reset()
        ));
        for id in &entry.added {
            if let Some(s) = seed.as_ref().and_then(|v| v.get(id)) {
                out.push_str(&format!("    + {}  ({})\n", model_summary(s), id));
            }
        }
    }

    if !entry.changed.is_empty() {
        out.push_str(&format!(
            "{}  Customized on disk (kept by merge):{}\n",
            STYLE_ERR.render(),
            STYLE_ERR.render_reset()
        ));
        for id in &entry.changed {
            let d = disk.as_ref().and_then(|v| v.get(id));
            let s = seed.as_ref().and_then(|v| v.get(id));
            let label = s
                .or(d)
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or(id.as_str())
                .to_string();
            out.push_str(&format!("    ~ {}  ({})\n", label, id));
            match (d, s) {
                (Some(dv), Some(sv)) => out.push_str(&diff_model_fields(dv, sv)),
                _ => out.push_str("      (unable to compare values)\n"),
            }
        }
    }
    out
}

/// One-line human summary of a model definition value.
fn model_summary(v: &Value) -> String {
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("?");
    let ctx = v.get("context_window").and_then(|x| x.as_u64()).unwrap_or(0);
    let price = v
        .get("cost")
        .and_then(|c| c.get("input").and_then(|x| x.as_f64()))
        .zip(v.get("cost").and_then(|c| c.get("output").and_then(|x| x.as_f64())))
        .map(|(inp, out)| format!("${inp:.2}/${out:.2} per M"))
        .unwrap_or_default();
    let mut s = name.to_string();
    if ctx > 0 {
        s.push_str(&format!(" · {ctx} ctx"));
    }
    if !price.is_empty() {
        s.push_str(&format!(" · {price}"));
    }
    s
}

/// Compact single-value rendering (no raw JSON dumps).
fn short_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("\"{s}\""),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Array(_) => "<array>".into(),
        Value::Object(_) => "<object>".into(),
    }
}

/// Field-level differences between two model definitions, as a string.
fn diff_model_fields(disk: &Value, seed: &Value) -> String {
    let mut out = String::new();
    let (Some(d), Some(s)) = (disk.as_object(), seed.as_object()) else {
        out.push_str("      (unable to compare values)\n");
        return out;
    };

    // Union of keys, stable order.
    let mut keys: Vec<&String> = d.keys().collect();
    for k in s.keys() {
        if !keys.contains(&k) {
            keys.push(k);
        }
    }

    for k in keys {
        let dv = d.get(k);
        let sv = s.get(k);
        match (dv, sv) {
            (Some(a), Some(b)) if a == b => continue,
            (Some(a), Some(b)) => {
                if k.as_str() == "cost" && a.is_object() && b.is_object() {
                    let da = a.as_object().unwrap();
                    let sb = b.as_object().unwrap();
                    for ck in ["input", "output", "cache_read", "cache_write"] {
                        let dprice = da.get(ck).and_then(|x| x.as_f64()).unwrap_or(0.0);
                        let sprice = sb.get(ck).and_then(|x| x.as_f64()).unwrap_or(0.0);
                        if (dprice - sprice).abs() > f64::EPSILON {
                            out.push_str(&format!("      cost.{ck}:  ${dprice:.2} → ${sprice:.2}\n"));
                        }
                    }
                    if da.get("tiers") != sb.get("tiers") {
                        out.push_str("      cost.tiers:  <differ>\n");
                    }
                    continue;
                }
                if k.as_str() == "thinking_level_map" {
                    out.push_str("      thinking:  <levels differ>\n");
                    continue;
                }
                out.push_str(&format!("      {}:  {} → {}\n", k, short_value(a), short_value(b)));
            }
            (Some(a), None) => out.push_str(&format!("      {}:  {}  (removed on disk)\n", k, short_value(a))),
            (None, Some(b)) => out.push_str(&format!("      {}:  (missing on disk) → {}\n", k, short_value(b))),
            (None, None) => {}
        }
    }
    out
}

#[cfg(test)]
mod diff_tests {
    use super::*;

    #[test]
    fn diff_is_field_level_not_raw_json() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();

        // Start from the embedded seed, then customize a few fields on disk.
        let seed_str = elph_ai::embedded_provider_json("anthropic").expect("embedded seed");
        let mut seed: serde_json::Value = serde_json::from_str(&seed_str).unwrap();
        let m = seed.get_mut("claude-opus-4-5").expect("model present");
        m["name"] = serde_json::json!("Custom Opus");
        m["context_window"] = serde_json::json!(123456);
        m["cost"]["input"] = serde_json::json!(9.99);
        std::fs::write(dir.join("anthropic.json"), serde_json::to_string(&seed).unwrap()).unwrap();

        let entry = elph_ai::ProviderUpdatePlanEntry {
            provider: "anthropic".into(),
            status: elph_ai::ProviderUpdateStatus::Conflict,
            added: vec![],
            changed: vec!["claude-opus-4-5".into()],
            unparsable: false,
        };

        let out = format_diff_entry(&dir, &entry);
        assert!(out.contains("Custom Opus"), "names the model: {out}");
        assert!(out.contains("name:"), "shows changed name field: {out}");
        assert!(out.contains("context_window:"), "shows context_window field: {out}");
        assert!(out.contains("cost.input:"), "shows cost.input field: {out}");
        assert!(!out.contains("\"context_window\":"), "must not dump raw JSON: {out}");
        assert!(!out.contains("\"id\":"), "must not dump raw JSON: {out}");
    }
}

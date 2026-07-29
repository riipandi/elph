use clap::{Parser, Subcommand};

use super::help;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};
use crate::utils::path::AppPaths;

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
    List {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Sign in to an AI provider (opens TUI for interactive login)
    Connect {
        /// Provider ID to connect (e.g. anthropic, openai-codex, github-copilot)
        provider: Option<String>,
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
        ProviderCommands::Connect { provider } => handle_connect(provider.as_deref()),
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

fn handle_list(json: &bool) -> ExitCode {
    let paths = match resolve_paths() {
        Ok(p) => p,
        Err(e) => return e,
    };

    let auth_store_path = paths.auth_store_path();
    let provider_ids = crate::tui::provider_credential_store::list_providers_with_credentials(&auth_store_path);

    if *json {
        println!("{}", serde_json::to_string_pretty(&provider_ids).unwrap_or_else(|_| "[]".into()));
        return EXIT_SUCCESS;
    }

    if provider_ids.is_empty() {
        println!("No stored provider credentials.");
        return EXIT_SUCCESS;
    }

    for id in &provider_ids {
        let name = crate::tui::provider_connect_dialog::format_provider_name(id);
        println!("{id}  ({name})");
    }
    EXIT_SUCCESS
}

fn handle_connect(provider: Option<&str>) -> ExitCode {
    help::unimplemented(&format!(
        "Provider connect — use `elph` TUI /provider connect{} (interactive login not yet available in CLI)",
        provider.map_or(String::new(), |p| format!(" {p}"))
    ));
    EXIT_SUCCESS
}

fn handle_disconnect(provider: Option<&str>) -> ExitCode {
    let paths = match resolve_paths() {
        Ok(p) => p,
        Err(e) => return e,
    };

    let auth_store_path = paths.auth_store_path();
    let provider_ids = crate::tui::provider_credential_store::list_providers_with_credentials(&auth_store_path);

    if let Some(pid) = provider {
        if !provider_ids.contains(&pid.to_string()) {
            println!("No stored credentials for provider '{pid}'.");
            return EXIT_SUCCESS;
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        match rt.block_on(crate::tui::provider_credential_store::delete_provider_credential(&auth_store_path, pid)) {
            Ok(true) => {
                println!("Signed out from {pid}.");
                EXIT_SUCCESS
            }
            Ok(false) => {
                println!("No stored credentials for provider '{pid}'.");
                EXIT_SUCCESS
            }
            Err(e) => {
                eprintln!("Failed to disconnect provider '{pid}': {e}");
                EXIT_ERROR
            }
        }
    } else {
        // Disconnect all
        if provider_ids.is_empty() {
            println!("No stored provider credentials to disconnect.");
            return EXIT_SUCCESS;
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        let mut removed = 0usize;
        let mut errors = 0usize;
        for pid in &provider_ids {
            match rt.block_on(crate::tui::provider_credential_store::delete_provider_credential(&auth_store_path, pid)) {
                Ok(true) => {
                    removed += 1;
                }
                Ok(false) => {
                    // already gone
                }
                Err(e) => {
                    eprintln!("Failed to disconnect provider '{pid}': {e}");
                    errors += 1;
                }
            }
        }

        if errors > 0 {
            eprintln!("Disconnected {removed} provider(s), {errors} error(s).");
            EXIT_ERROR
        } else {
            println!("Signed out from all providers ({removed}).");
            EXIT_SUCCESS
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

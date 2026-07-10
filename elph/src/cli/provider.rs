use clap::{Parser, Subcommand};
use elph_ai::builtin_models;

use super::help;
use crate::agent::{provider_api_key_env, provider_config};
use crate::platform::{EXIT_SUCCESS, ExitCode};

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
    /// List configured providers and credentials
    List {
        /// Emit machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Connect to an AI provider (interactive login)
    Connect {
        /// Provider name or URL to connect to
        provider: Option<String>,
    },
    /// Disconnect from an AI provider and clear credentials
    Disconnect {
        /// Provider name to disconnect (disconnects all if omitted)
        provider: Option<String>,
    },
    /// Import providers from a custom registry (api.json)
    Add {
        /// URL or path to a provider registry
        url: String,
    },
    /// Remove a provider and its model aliases
    Remove {
        /// Provider ID to remove
        provider_id: String,
    },
    /// Discover and import providers from the public models.dev catalog
    Catalog,
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
        ProviderCommands::List { json } => {
            if *json {
                help::unimplemented("Provider list --json not yet implemented");
                return EXIT_SUCCESS;
            }
            let models = builtin_models(None).into_arc();
            for provider in models.get_providers() {
                let id = &provider.id;
                let label = provider_config(id).map(|c| c.label).unwrap_or(id);
                let status = if let Some(model) = provider.get_models().first() {
                    match elph_agent::block_on(models.get_auth(model)) {
                        Ok(Some(_)) => "authenticated",
                        _ => "missing",
                    }
                } else {
                    "no-models"
                };
                let env = provider_api_key_env(id).unwrap_or("—");
                println!("{id}  {label}  {status}  ({env})");
            }
            EXIT_SUCCESS
        }
        ProviderCommands::Connect { provider } => {
            help::unimplemented(&format!(
                "Provider connect — use `elph` TUI /provider connect (provider: {})",
                provider.as_deref().unwrap_or("<interactive>")
            ));
            EXIT_SUCCESS
        }
        ProviderCommands::Disconnect { provider } => {
            help::unimplemented(&format!(
                "Provider disconnect — use `elph` TUI /provider disconnect (provider: {})",
                provider.as_deref().unwrap_or("<all>")
            ));
            EXIT_SUCCESS
        }
        ProviderCommands::Add { url } => {
            help::unimplemented(&format!("Provider add — not yet implemented (url: {url})"));
            EXIT_SUCCESS
        }
        ProviderCommands::Remove { provider_id } => {
            help::unimplemented(&format!(
                "Provider remove — not yet implemented (provider_id: {provider_id})"
            ));
            EXIT_SUCCESS
        }
        ProviderCommands::Catalog => {
            help::unimplemented("Provider catalog — not yet implemented");
            EXIT_SUCCESS
        }
        ProviderCommands::Update { provider_id } => {
            help::unimplemented(&format!(
                "Provider update — not yet implemented (provider_id: {})",
                provider_id.as_deref().unwrap_or("<all>")
            ));
            EXIT_SUCCESS
        }
    }
}

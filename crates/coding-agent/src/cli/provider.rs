use clap::{Args, Subcommand};

use crate::app::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
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
        eprintln!("Manage AI providers and credentials");
        eprintln!();
        eprintln!("Usage: elph provider <command>");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  list        List configured providers and credentials");
        eprintln!("  connect     Connect to an AI provider (interactive login)");
        eprintln!("  disconnect  Disconnect from an AI provider and clear credentials");
        eprintln!("  add         Import providers from a custom registry (api.json)");
        eprintln!("  remove      Remove a provider and its model aliases");
        eprintln!("  catalog     Discover and import providers from models.dev");
        eprintln!("  update      Update provider metadata and credentials");
        eprintln!("  help        Print this message or the help of the given subcommand(s)");
        return EXIT_SUCCESS;
    };
    match cmd {
        ProviderCommands::List { json } => {
            eprintln!("Provider list — not yet implemented (json: {json})");
            EXIT_SUCCESS
        }
        ProviderCommands::Connect { provider } => {
            eprintln!(
                "Provider connect — not yet implemented (provider: {})",
                provider.as_deref().unwrap_or("<interactive>")
            );
            EXIT_SUCCESS
        }
        ProviderCommands::Disconnect { provider } => {
            eprintln!(
                "Provider disconnect — not yet implemented (provider: {})",
                provider.as_deref().unwrap_or("<all>")
            );
            EXIT_SUCCESS
        }
        ProviderCommands::Add { url } => {
            eprintln!("Provider add — not yet implemented (url: {url})");
            EXIT_SUCCESS
        }
        ProviderCommands::Remove { provider_id } => {
            eprintln!("Provider remove — not yet implemented (provider_id: {provider_id})");
            EXIT_SUCCESS
        }
        ProviderCommands::Catalog => {
            eprintln!("Provider catalog — not yet implemented");
            EXIT_SUCCESS
        }
        ProviderCommands::Update { provider_id } => {
            eprintln!(
                "Provider update — not yet implemented (provider_id: {})",
                provider_id.as_deref().unwrap_or("<all>")
            );
            EXIT_SUCCESS
        }
    }
}

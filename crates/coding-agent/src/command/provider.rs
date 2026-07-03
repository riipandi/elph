use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::{ProviderArgs, ProviderCommands};

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

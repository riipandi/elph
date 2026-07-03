use clap::{Args, Subcommand};

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

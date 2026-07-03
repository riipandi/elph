use clap::{Args, Subcommand};

#[derive(Args, Default)]
pub struct ServerArgs {
    #[command(subcommand)]
    pub command: Option<ServerCommands>,

    /// Port to listen on
    #[arg(short, long, default_value_t = 8080)]
    pub port: u16,

    /// Hostname to bind to
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
}

#[derive(Subcommand)]
pub enum ServerCommands {
    /// Start the Elph server (background daemon; use --foreground to attach)
    Run(ServerRunArgs),
    /// List clients currently connected to the running Elph server
    Ps,
    /// Stop the running Elph server
    Kill,
    /// Generate a new persistent server token
    RotateToken,
}

#[derive(Args, Default)]
pub struct ServerRunArgs {
    /// Run in the foreground instead of as a background daemon
    #[arg(long)]
    pub foreground: bool,
}

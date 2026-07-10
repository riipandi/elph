use clap::{Args, Parser, Subcommand};

use super::help;
use crate::platform::{EXIT_SUCCESS, ExitCode};

#[derive(Parser, Default)]
#[command(
    name = "server",
    about = "Run the local Elph server (REST + WebSocket + web UI)",
    color = clap::ColorChoice::Auto
)]
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

pub fn handle(args: &ServerArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<ServerArgs>();
    };
    match cmd {
        ServerCommands::Run(run_args) => {
            help::unimplemented(&format!(
                "Server run — not yet implemented (port: {}, host: {}, foreground: {})",
                args.port, args.host, run_args.foreground
            ));
            EXIT_SUCCESS
        }
        ServerCommands::Ps => {
            help::unimplemented("Server ps — not yet implemented");
            EXIT_SUCCESS
        }
        ServerCommands::Kill => {
            help::unimplemented("Server kill — not yet implemented");
            EXIT_SUCCESS
        }
        ServerCommands::RotateToken => {
            help::unimplemented("Server rotate-token — not yet implemented");
            EXIT_SUCCESS
        }
    }
}

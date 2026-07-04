use clap::{Args, Subcommand};

use crate::runtime::{EXIT_SUCCESS, ExitCode};

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

pub fn handle(args: &ServerArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        eprintln!("Run the local Elph server (REST + WebSocket + web UI)");
        eprintln!();
        eprintln!("Usage: elph server [OPTIONS] [COMMAND]");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  run           Start the Elph server");
        eprintln!("  ps            List connected clients");
        eprintln!("  kill          Stop the running server");
        eprintln!("  rotate-token  Generate a new persistent server token");
        eprintln!("  help          Print this message or the help of the given subcommand(s)");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  -p, --port <PORT>  Port to listen on [default: 8080]");
        eprintln!("      --host <HOST>  Hostname to bind to [default: 127.0.0.1]");
        return EXIT_SUCCESS;
    };
    match cmd {
        ServerCommands::Run(run_args) => {
            eprintln!(
                "Server run — not yet implemented (port: {}, host: {}, foreground: {})",
                args.port, args.host, run_args.foreground
            );
            EXIT_SUCCESS
        }
        ServerCommands::Ps => {
            eprintln!("Server ps — not yet implemented");
            EXIT_SUCCESS
        }
        ServerCommands::Kill => {
            eprintln!("Server kill — not yet implemented");
            EXIT_SUCCESS
        }
        ServerCommands::RotateToken => {
            eprintln!("Server rotate-token — not yet implemented");
            EXIT_SUCCESS
        }
    }
}

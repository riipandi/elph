use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::{ServerArgs, ServerCommands};

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

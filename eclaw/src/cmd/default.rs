use crate::runtime::{EXIT_ERROR, EXIT_SUCCESS, ExitCode};
use crate::server::{ServerConfig, run_blocking};

/// Start the HTTP server (default, no subcommand).
pub fn handle(cli: &super::Cli) -> ExitCode {
    let config = ServerConfig::new(&cli.host, cli.port);
    if let Err(err) = run_blocking(config) {
        tracing::error!(error = %err, "server error");
        return EXIT_ERROR;
    }

    EXIT_SUCCESS
}

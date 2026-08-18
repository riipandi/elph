use clap::Args;

use crate::platform::acp::AcpMode;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Settings};
use crate::platform::{ensure_datastore_blocking, ensure_home_blocking};

/// Run Elph as an ACP agent.
#[derive(Debug, Args)]
pub struct AcpArgs {
    /// Speak ACP over stdio (the only supported transport).
    #[arg(long)]
    pub stdio: bool,

    /// Use experimental ACP v2 (requires `--stdio`).
    #[arg(long, requires = "stdio")]
    pub experimental: bool,

    /// Interactive provider login (ACP Terminal Auth). Does not start stdio.
    #[arg(long, conflicts_with_all = ["stdio", "experimental"])]
    pub setup: bool,
}

pub fn handle(args: &AcpArgs) -> ExitCode {
    if args.setup {
        return crate::cli::provider::run_interactive_connect();
    }
    let mode = if args.experimental { AcpMode::V2 } else { AcpMode::V1 };
    let paths = match ensure_home_blocking(env!("CARGO_PKG_VERSION")) {
        Ok(paths) => paths,
        Err(error) => {
            log::error!("ACP home bootstrap failed: {error:#}");
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };

    let settings = match Settings::load(&paths) {
        Ok(settings) => {
            settings.apply_http_proxy_env();
            settings
        }
        Err(error) => {
            log::error!("ACP settings load failed: {error:#}");
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };

    // Initialize datastore before starting the server so session/new
    // does not block on database creation for every request.
    if let Err(error) = ensure_datastore_blocking(&paths) {
        log::error!("ACP datastore init failed: {error:#}");
        eprintln!("failed to initialize datastore: {error}");
        return EXIT_ERROR;
    }

    log::info!("ACP server starting mode={mode:?}");
    match elph_agent::runtime::try_block_on(crate::platform::acp::run_agent_stdio(paths, settings, mode)) {
        Ok(Ok(())) => EXIT_SUCCESS,
        Ok(Err(error)) => {
            log::error!("ACP server error: {error:#}");
            eprintln!("ACP server error: {error}");
            EXIT_ERROR
        }
        Err(error) => {
            log::error!("ACP runtime start failed: {error:#}");
            eprintln!("failed to start runtime: {error}");
            EXIT_ERROR
        }
    }
}

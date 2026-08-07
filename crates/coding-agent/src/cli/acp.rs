use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Settings};
use crate::platform::{ensure_datastore_blocking, ensure_home_blocking};

pub fn handle() -> ExitCode {
    let paths = match ensure_home_blocking(env!("CARGO_PKG_VERSION")) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };

    let settings = match Settings::load(&paths) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            return EXIT_ERROR;
        }
    };

    // Initialize datastore before starting the server so session/new
    // does not block on database creation for every request.
    if let Err(error) = ensure_datastore_blocking(&paths) {
        eprintln!("failed to initialize datastore: {error}");
        return EXIT_ERROR;
    }

    match elph_agent::try_block_on(crate::platform::acp::run_agent_stdio(paths, settings)) {
        Ok(Ok(())) => EXIT_SUCCESS,
        Ok(Err(error)) => {
            eprintln!("ACP server error: {error}");
            EXIT_ERROR
        }
        Err(error) => {
            eprintln!("failed to start runtime: {error}");
            EXIT_ERROR
        }
    }
}

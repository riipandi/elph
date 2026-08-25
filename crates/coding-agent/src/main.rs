use anyhow::Result;
use clap::Parser;

use elph::cli;
use elph::env;

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = cli::Cli::parse();

    if cli_args.version {
        std::process::exit(cli::version::handle());
    }

    // Load and apply environment variables based on CLI arguments
    let env_vars = env::load_environment(cli_args.no_global_env, cli_args.env_file.as_ref())?;
    env::apply_environment(&env_vars, cli_args.no_global_env)?;

    let code = cli::run(&cli_args);
    std::process::exit(code);
}

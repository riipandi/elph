mod acp;
mod completions;
mod default;
mod doctor;
mod export;
mod import;
mod mcp;
mod models;
mod provider;
mod server;
mod session;
mod stats;
mod update;
pub mod version;
mod worktree;

use crate::app::ExitCode;
use crate::cli;

pub fn run(cli: &cli::Cli) -> ExitCode {
    use cli::Commands;

    let Some(cmd) = &cli.command else {
        return default::handle();
    };

    match cmd {
        Commands::Acp => acp::handle(),
        Commands::Completions => completions::handle(),
        Commands::Doctor => doctor::handle(),
        Commands::Export => export::handle(),
        Commands::Import => import::handle(),
        Commands::Mcp => mcp::handle(),
        Commands::Models => models::handle(),
        Commands::Provider => provider::handle(),
        Commands::Server => server::handle(),
        Commands::Session => session::handle(),
        Commands::Stats => stats::handle(),
        Commands::Update(args) => update::handle(args),
        Commands::Version => version::handle(),
        Commands::Worktree => worktree::handle(),
    }
}

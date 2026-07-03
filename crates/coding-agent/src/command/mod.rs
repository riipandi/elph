mod acp;
mod completions;
mod default;
mod doctor;
mod export;
mod import;
mod mcp;
mod models;
mod plugin;
mod provider;
mod run;
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
        Commands::Completions(args) => completions::handle(args),
        Commands::Doctor(args) => doctor::handle(args),
        Commands::Export(args) => export::handle(args),
        Commands::Import(args) => import::handle(args),
        Commands::Mcp(args) => mcp::handle(args),
        Commands::Models(args) => models::handle(args),
        Commands::Plugin(args) => plugin::handle(args),
        Commands::Provider(args) => provider::handle(args),
        Commands::Run(args) => run::handle(args),
        Commands::Server(args) => server::handle(args),
        Commands::Session(args) => session::handle(args),
        Commands::Stats(args) => stats::handle(args),
        Commands::Update(args) => update::handle(args),
        Commands::Version => version::handle(),
        Commands::Worktree(args) => worktree::handle(args),
    }
}

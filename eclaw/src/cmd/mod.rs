mod default;
mod doctor;
pub mod version;

use crate::runtime::ExitCode;
use clap::{Parser, Subcommand};

pub use doctor::DoctorArgs;

/// Personal AI assistant powered by Elph
#[derive(Parser)]
#[command(name = "eclaw", about, disable_version_flag = true)]
pub struct Cli {
    /// Print version information
    #[arg(short = 'V', long = "version", help = "Print version information")]
    pub version: bool,

    /// Port to listen on
    #[arg(short, long, default_value_t = 32529)]
    pub port: u16,

    /// Hostname to bind to
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show the configuration Eclaw discovers for this machine
    Doctor(DoctorArgs),
    /// Print version information
    Version,
}

pub fn run(cli: &Cli) -> ExitCode {
    if let Err(err) = crate::layout::ensure_blocking(env!("CARGO_PKG_VERSION")) {
        eprintln!("failed to initialize eclaw home: {err}");
        return crate::runtime::EXIT_ERROR;
    }

    match &cli.command {
        None => default::handle(cli),
        Some(Commands::Doctor(args)) => doctor::handle(args),
        Some(Commands::Version) => version::handle(),
    }
}

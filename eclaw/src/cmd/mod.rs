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

fn init_layout() -> Result<crate::layout::Paths, ExitCode> {
    crate::layout::ensure_layout_blocking(env!("CARGO_PKG_VERSION")).map_err(|err| {
        eprintln!("failed to initialize eclaw home: {err}");
        crate::runtime::EXIT_ERROR
    })
}

fn init_datastore(paths: &crate::layout::Paths) -> Result<(), ExitCode> {
    crate::layout::ensure_datastore_blocking(paths).map_err(|err| {
        eprintln!("failed to initialize eclaw databases: {err}");
        crate::runtime::EXIT_ERROR
    })
}

pub fn run(cli: &Cli) -> ExitCode {
    let paths = match init_layout() {
        Ok(paths) => paths,
        Err(code) => return code,
    };

    match &cli.command {
        None => {
            if let Err(code) = init_datastore(&paths) {
                return code;
            }
            default::handle(cli)
        }
        Some(Commands::Doctor(args)) => doctor::handle(args),
        Some(Commands::Version) => version::handle(),
    }
}

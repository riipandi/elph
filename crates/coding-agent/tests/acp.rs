//! ACP CLI + library linkage.

use clap::Parser;
use elph::cli::{Cli, Commands};
use elph::platform::acp::AcpMode;

#[test]
fn acp_module_is_linked() {
    let _ = std::any::type_name_of_val(&elph::platform::acp::run_agent_stdio);
}

#[test]
fn bare_acp_is_v1() {
    let cli = Cli::try_parse_from(["elph", "acp"]).expect("parse");
    match cli.command {
        Some(Commands::Acp(args)) => {
            assert!(!args.stdio && !args.experimental);
            assert_eq!(mode(&args), AcpMode::V1);
        }
        _ => panic!("expected Commands::Acp"),
    }
}

#[test]
fn stdio_is_v1() {
    let cli = Cli::try_parse_from(["elph", "acp", "--stdio"]).expect("parse");
    match cli.command {
        Some(Commands::Acp(args)) => {
            assert!(args.stdio && !args.experimental);
            assert_eq!(mode(&args), AcpMode::V1);
        }
        _ => panic!("expected Commands::Acp"),
    }
}

#[test]
fn experimental_requires_stdio() {
    assert!(Cli::try_parse_from(["elph", "acp", "--experimental"]).is_err());
}

#[test]
fn stdio_experimental_is_v2() {
    let cli = Cli::try_parse_from(["elph", "acp", "--stdio", "--experimental"]).expect("parse");
    match cli.command {
        Some(Commands::Acp(args)) => {
            assert!(args.stdio && args.experimental);
            assert_eq!(mode(&args), AcpMode::V2);
        }
        _ => panic!("expected Commands::Acp"),
    }
}

fn mode(args: &elph::cli::AcpArgs) -> AcpMode {
    if args.experimental { AcpMode::V2 } else { AcpMode::V1 }
}

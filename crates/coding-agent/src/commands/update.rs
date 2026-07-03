use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::UpdateArgs;

pub fn handle(args: &UpdateArgs) -> ExitCode {
    println!("Update — not yet implemented");
    if args.check {
        println!("  --check: true");
    }
    if args.json {
        println!("  --json: true");
    }
    if args.force_reinstall {
        println!("  --force-reinstall: true");
    }
    if let Some(v) = &args.version {
        println!("  --version: {v}");
    }
    if args.canary {
        println!("  --canary: true");
    }
    if args.stable {
        println!("  --stable: true");
    }
    EXIT_SUCCESS
}

use crate::runtime::{EXIT_SUCCESS, ExitCode};

pub fn handle() -> ExitCode {
    println!("eclaw v{}", env!("CARGO_PKG_VERSION"));
    EXIT_SUCCESS
}

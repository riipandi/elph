mod app;
pub mod exit_message;
mod interrupt;

pub use app::{EXIT_ERROR, EXIT_INTERRUPTED, EXIT_SUCCESS, ExitCode, WAS_INTERRUPTED, run};
#[cfg(unix)]
pub use app::{SHOULD_KILL_PARENT, kill_parent};
pub use interrupt::handle_prompt_interrupt;

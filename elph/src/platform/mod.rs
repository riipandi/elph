pub mod acp;
mod agent_mode;
mod app;
pub mod bootstrap;
pub mod datastore;
pub mod exit_message;
mod hooks;
mod interrupt;
pub mod mcp;
mod migrations;
pub mod paths;
mod project;
mod session;
mod settings;

pub use app::{EXIT_ERROR, EXIT_INTERRUPTED, EXIT_SUCCESS, ExitCode, WAS_INTERRUPTED, run};
#[cfg(unix)]
pub use app::{SHOULD_KILL_PARENT, kill_parent};
pub use bootstrap::ensure_home_blocking;
pub use datastore::{ensure as ensure_datastore, ensure_blocking as ensure_datastore_blocking};
pub use elph_core::utils::path::AppPaths;
pub use interrupt::handle_prompt_interrupt;
pub use paths::Paths;
pub use project::ensure as ensure_project;
pub use settings::Settings;

pub mod acp;
mod agent_mode;
mod app;
pub mod bootstrap;
pub mod datastore;
pub mod exit_message;
pub mod hooks;
mod interrupt;
pub mod mcp;
pub mod migrations;
pub mod paths;
mod project;
pub mod scaffold;
mod session;
pub mod settings;

pub use crate::utils::path::AppPaths;
#[cfg(unix)]
pub use app::SHOULD_KILL_PARENT;
#[cfg(unix)]
pub use app::kill_parent;
pub use app::run;
pub use app::{EXIT_ERROR, EXIT_INTERRUPTED, EXIT_PERMISSION_DENIED, EXIT_SUCCESS, ExitCode, WAS_INTERRUPTED};
pub use bootstrap::ensure_home_blocking;
pub use datastore::{ensure as ensure_datastore, ensure_blocking as ensure_datastore_blocking};
pub use interrupt::PromptInterrupt;
pub use interrupt::{handle_prompt_interrupt, handle_prompt_interrupt_text};
pub use paths::Paths;
pub use project::ensure as ensure_project;
pub use scaffold::{DefaultProjectTrust, TrustStore};
pub use settings::{
    EmbedSettings, GpuAcceleration, MemorySettings, ModelsSettings, NotificationSettings, ResourcesSettings,
    SessionSettings, Settings, SettingsScope, UiSettings,
};

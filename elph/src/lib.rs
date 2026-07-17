pub mod agent;
pub mod cli;
pub mod command;
pub mod extensions;
pub mod logger;
pub mod memory;
pub mod platform;
pub mod prompt;
pub mod scaffold;
pub mod skills;
pub mod tui;
pub mod types;
pub mod utils;
pub mod worktree;

/// Process-global fastrace helpers (reporter lives in `elph-ai`; product `logger` installs it).
pub mod trace {
    pub use elph_ai::trace::*;
}

pub use scaffold::{BundledManifest, TrustStore, VersionFile};

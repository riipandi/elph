//! SuperLightTUI interactive shell for Owly.

mod app;
mod ask;
mod banner;
mod chat_stream;
mod chrome;
mod context;
mod entries;
mod launch;
mod setup;
mod slash;
mod static_flush;
mod tool_display;
mod transcript;

use anyhow::Result;

use crate::config::Config;
use crate::env;
use crate::onboarding;
use crate::session::SessionStore;
use crate::startup::{InitialRun, stdin_is_tty};

pub use context::AppContext;

/// Prepare credentials/session and launch the interactive Owly shell.
pub async fn run_interactive(
    config: &Config,
    cwd: &std::path::Path,
    stream: bool,
    verbose: bool,
    initial: Option<InitialRun>,
) -> Result<()> {
    let config = config.clone();
    let pending_setup = stdin_is_tty() && onboarding::needs_setup(&config);
    if !pending_setup {
        env::setup_environment(&config)?;
    }

    let session = SessionStore::open(cwd).await?;
    let loaded = session.load_conversation().await?;
    let restored_count = loaded.messages.len();
    let recovery = loaded.recovery;
    let db_path = session.db_path().to_path_buf();

    let launch = launch::from_session(launch::LaunchOptions {
        config,
        cwd: cwd.to_path_buf(),
        stream,
        verbose,
        pending_setup,
        session,
        restored_count,
        recovery,
        db_path,
        initial,
    });

    app::run_shell(launch).await
}

//! iocraft-based interactive shell (Elph-parity phase 1).

mod activity;
mod app;
mod banner;
mod chat_stream;
mod chrome;
mod context;
mod entries;
mod launch;
mod setup;
mod spinner;
mod transcript;

use anyhow::Result;
use elph_agent::try_block_on;
use iocraft::prelude::*;

use crate::config::Config;
use crate::env;
use crate::onboarding;
use crate::session::SessionStore;
use crate::startup::{InitialRun, stdin_is_tty};

pub use context::AppContext;

/// Prepare credentials/session and launch the interactive iocraft shell.
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
    let restored_count = session.load_messages().await?.len();
    let db_path = session.db_path().to_path_buf();

    launch::from_session(launch::LaunchOptions {
        config,
        cwd: cwd.to_path_buf(),
        stream,
        verbose,
        pending_setup,
        session,
        restored_count,
        db_path,
        initial,
    })
    .install();

    try_block_on(
        element!(app::OwlyRoot)
            .fullscreen()
            .disable_mouse_capture()
            .ignore_ctrl_c(),
    )??;
    Ok(())
}

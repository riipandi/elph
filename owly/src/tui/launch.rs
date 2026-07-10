//! One-shot launch payload for the Owly interactive shell.

use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::session::{SessionRecovery, SessionStore};

use super::context::AppContext;

/// Data required to boot the interactive Owly application.
pub struct LaunchState {
    pub app_context: AppContext,
    pub provider: String,
    pub model: String,
    pub session_id: String,
    pub pending_setup: bool,
    pub startup_lines: Vec<String>,
    pub initial: Option<String>,
    pub submit_tx: mpsc::UnboundedSender<String>,
    pub submit_rx: Option<mpsc::UnboundedReceiver<String>>,
}

/// Inputs for constructing [`LaunchState`].
pub struct LaunchOptions {
    pub config: Config,
    pub cwd: PathBuf,
    pub stream: bool,
    pub verbose: bool,
    pub pending_setup: bool,
    pub session: SessionStore,
    pub restored_count: usize,
    pub recovery: SessionRecovery,
    pub db_path: PathBuf,
    pub initial: Option<crate::startup::InitialRun>,
}

/// Build launch state from resolved interactive session options.
pub fn from_session(opts: LaunchOptions) -> LaunchState {
    let provider = opts.config.provider.clone();
    let model = opts.config.model_id.clone();
    let session_id = opts.session.thread_id().to_string();
    let app_context = AppContext::new(opts.config, opts.cwd, opts.stream, opts.verbose, opts.session);
    let startup_lines = crate::shell::startup_transcript_lines(opts.restored_count, &opts.recovery, &opts.db_path);
    let initial = opts.initial.map(crate::shell::initial_input);
    let (submit_tx, submit_rx) = mpsc::unbounded_channel();

    LaunchState {
        app_context,
        provider,
        model,
        session_id,
        pending_setup: opts.pending_setup,
        startup_lines,
        initial,
        submit_tx,
        submit_rx: Some(submit_rx),
    }
}

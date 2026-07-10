//! Interactive TUI application shell.

mod app;

pub use app::{ElphApp, render_app, run_sigint_watcher, run_tui};

/// Launch options for the interactive TUI.
#[derive(Debug, Clone, Default)]
pub struct TuiOptions {
    pub resume_id: Option<String>,
}

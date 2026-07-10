//! Interactive TUI application shell.

mod app;

pub use app::{ElphApp, render_app, run_sigint_watcher, run_tui};

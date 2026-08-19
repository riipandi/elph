//! Configurable local shell and PTY execution.
//!
//! Feature: `tools-shell-exec` — gated behind the feature flag to keep
//! the core runtime dependency-free for library consumers that only
//! need session/agent logic.
//!
//! ## Re-exports
//!
//! These are re-exported at `crate::` level under the `exec` namespace
//! and also via `crate::` root when `tools-shell-exec` is enabled:
//!
//! ```
//! use elph_agent::exec::{ShellConfig, exec_shell_command, ExecError};
//! ```

mod error;
mod output;
mod process;
pub mod pty;
mod shell;
mod types;

pub use error::{ExecError, ExecErrorCode, Result};
pub use output::sanitize_binary_output;
pub use shell::{exec_shell_command, resolve_shell};
pub use types::{ShellConfig, ShellExecOptions, ShellExecResult, ShellOutputCallback};

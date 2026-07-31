//! Configurable shell execution types.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

#[cfg(unix)]
use super::pty::PtySize;

/// Streaming chunk callback for stdout/stderr during shell execution.
pub type ShellOutputCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Resolved shell invocation defaults and PTY preferences.
#[derive(Debug, Clone)]
pub struct ShellConfig {
    /// When set, must exist on disk or [`ExecErrorCode::ShellUnavailable`] is returned.
    pub shell_path: Option<PathBuf>,
    /// Arguments passed before the command string (default: `["-c"]`).
    pub shell_args: Vec<String>,
    /// Base environment merged into every invocation (overridden per-run `env`).
    pub base_env: Option<HashMap<String, String>>,
    /// PTY geometry when [`Self::prefer_pty`] is enabled.
    #[cfg(unix)]
    pub pty_size: PtySize,
    /// Try PTY execution on Unix before falling back to piped `sh -c`.
    #[cfg(unix)]
    pub prefer_pty: bool,
    /// Read buffer size for streaming PTY output.
    pub read_chunk_bytes: usize,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            shell_path: None,
            shell_args: vec!["-c".to_string()],
            base_env: None,
            #[cfg(unix)]
            pty_size: PtySize::new(24, 120),
            #[cfg(unix)]
            prefer_pty: true,
            read_chunk_bytes: 4096,
        }
    }
}

impl ShellConfig {
    pub fn with_shell_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.shell_path = Some(path.into());
        self
    }

    pub fn with_shell_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.shell_args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_base_env(mut self, env: HashMap<String, String>) -> Self {
        self.base_env = Some(env);
        self
    }

    #[cfg(unix)]
    pub fn with_pty_size(mut self, size: PtySize) -> Self {
        self.pty_size = size;
        self
    }

    #[cfg(unix)]
    pub fn with_prefer_pty(mut self, prefer_pty: bool) -> Self {
        self.prefer_pty = prefer_pty;
        self
    }
}

/// Per-invocation shell execution options.
#[derive(Clone, Default)]
pub struct ShellExecOptions {
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
    /// Timeout in whole seconds.
    pub timeout: Option<u64>,
    pub abort_token: Option<CancellationToken>,
    pub on_stdout: Option<ShellOutputCallback>,
    pub on_stderr: Option<ShellOutputCallback>,
    /// Override PTY size for this invocation (Unix only).
    #[cfg(unix)]
    pub pty_size: Option<PtySize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

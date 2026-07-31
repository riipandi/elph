//! Shell and PTY execution errors.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecErrorCode {
    Aborted,
    Timeout,
    ShellUnavailable,
    SpawnError,
    CallbackError,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecError {
    pub code: ExecErrorCode,
    pub message: String,
}

impl ExecError {
    pub fn new(code: ExecErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn aborted() -> Self {
        Self::new(ExecErrorCode::Aborted, "aborted")
    }
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExecError {}

pub type Result<T> = std::result::Result<T, ExecError>;

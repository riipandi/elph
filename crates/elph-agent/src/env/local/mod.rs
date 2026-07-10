//! Local filesystem and shell execution environment.

mod filesystem;
mod shell;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::fs;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::harness::types::{
    CreateDirOptions, ExecutionError, ExecutionErrorCode, FileError, FileErrorCode, FileInfo, FileKind, FileSystem,
    Result, err, ok,
};

const MAX_TIMEOUT_MS: u64 = 2_147_483_647;
const MAX_TIMEOUT_SECONDS: u64 = MAX_TIMEOUT_MS / 1000;

/// Local filesystem execution environment for tests and local tooling.
pub struct LocalExecutionEnv {
    cwd: PathBuf,
    shell_path: Option<PathBuf>,
    shell_env: Option<HashMap<String, String>>,
}

impl LocalExecutionEnv {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            shell_path: None,
            shell_env: None,
        }
    }

    pub fn with_shell_path(mut self, shell_path: impl Into<PathBuf>) -> Self {
        self.shell_path = Some(shell_path.into());
        self
    }

    pub fn with_shell_env(mut self, shell_env: HashMap<String, String>) -> Self {
        self.shell_env = Some(shell_env);
        self
    }

    /// Convenience helper for tests: create a directory.
    pub async fn create_dir(&self, path: &str, recursive: bool) -> Result<(), FileError> {
        FileSystem::create_dir(
            self,
            path,
            Some(CreateDirOptions {
                recursive,
                abort_token: None,
            }),
        )
        .await
    }

    /// Convenience helper for tests: write UTF-8 text to a file.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), FileError> {
        FileSystem::write_file(self, path, content.as_bytes(), None).await
    }

    pub(crate) fn normalize_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        }
    }

    fn abort_file_error<T>(token: Option<&CancellationToken>, path: Option<&str>) -> Option<Result<T, FileError>> {
        if token.is_some_and(|t| t.is_cancelled()) {
            Some(err(FileError::new(
                FileErrorCode::Aborted,
                "aborted",
                path.map(str::to_string),
            )))
        } else {
            None
        }
    }

    fn to_file_error(error: std::io::Error, path: Option<&str>) -> FileError {
        let message = error.to_string();
        let code = match error.kind() {
            std::io::ErrorKind::NotFound => FileErrorCode::NotFound,
            std::io::ErrorKind::PermissionDenied => FileErrorCode::PermissionDenied,
            std::io::ErrorKind::NotADirectory => FileErrorCode::NotDirectory,
            std::io::ErrorKind::IsADirectory => FileErrorCode::IsDirectory,
            std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => FileErrorCode::Invalid,
            std::io::ErrorKind::Unsupported => FileErrorCode::NotSupported,
            _ => FileErrorCode::Unknown,
        };
        FileError::new(code, message, path.map(str::to_string))
    }

    async fn metadata_for_path(&self, path: &Path) -> Result<FileInfo, FileError> {
        let normalized = Self::normalize_path(path);
        let metadata = match fs::symlink_metadata(path).await {
            Ok(metadata) => metadata,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        };

        let kind = if metadata.is_symlink() {
            FileKind::Symlink
        } else if metadata.is_dir() {
            FileKind::Directory
        } else if metadata.is_file() {
            FileKind::File
        } else {
            return err(FileError::new(
                FileErrorCode::Invalid,
                "Unsupported file type",
                Some(normalized),
            ));
        };

        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();

        let mtime_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);

        ok(FileInfo {
            name,
            path: normalized,
            kind,
            size: metadata.len(),
            mtime_ms,
        })
    }

    fn resolve_timeout_ms(timeout: Option<u64>) -> Result<Option<u64>, ExecutionError> {
        let Some(timeout) = timeout else {
            return ok(None);
        };
        if timeout == 0 {
            return err(ExecutionError::new(
                ExecutionErrorCode::Timeout,
                "Invalid timeout: must be a finite number of seconds",
            ));
        }
        let timeout_ms = timeout.saturating_mul(1000);
        if timeout_ms > MAX_TIMEOUT_MS {
            return err(ExecutionError::new(
                ExecutionErrorCode::Timeout,
                format!("Invalid timeout: maximum is {MAX_TIMEOUT_SECONDS} seconds"),
            ));
        }
        ok(Some(timeout_ms))
    }

    async fn path_exists(path: &Path) -> bool {
        fs::metadata(path).await.is_ok()
    }

    async fn find_bash_on_path() -> Option<PathBuf> {
        let output = Command::new("which").arg("bash").output().await.ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let first = stdout.lines().next()?.trim();
        if first.is_empty() {
            return None;
        }
        let path = PathBuf::from(first);
        if Self::path_exists(&path).await {
            Some(path)
        } else {
            None
        }
    }

    fn invoke_output_callback(
        callback: &std::sync::Arc<dyn Fn(&str) + Send + Sync>,
        chunk: &str,
    ) -> Option<ExecutionError> {
        let chunk = chunk.to_string();
        let callback = std::sync::Arc::clone(callback);
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            callback(&chunk);
        })) {
            Ok(()) => None,
            Err(payload) => {
                let message = if let Some(message) = payload.downcast_ref::<&str>() {
                    (*message).to_string()
                } else if let Some(message) = payload.downcast_ref::<String>() {
                    message.clone()
                } else {
                    "callback failed".to_string()
                };
                Some(ExecutionError::new(ExecutionErrorCode::CallbackError, message))
            }
        }
    }

    async fn get_shell_config(&self) -> Result<(PathBuf, Vec<String>), ExecutionError> {
        if let Some(shell_path) = &self.shell_path {
            if Self::path_exists(shell_path).await {
                return ok((shell_path.clone(), vec!["-c".to_string()]));
            }
            return err(ExecutionError::new(
                ExecutionErrorCode::ShellUnavailable,
                format!("Custom shell path not found: {}", shell_path.display()),
            ));
        }

        let bash = PathBuf::from("/bin/bash");
        if Self::path_exists(&bash).await {
            return ok((bash, vec!["-c".to_string()]));
        }
        if let Some(path) = Self::find_bash_on_path().await {
            return ok((path, vec!["-c".to_string()]));
        }
        ok((PathBuf::from("/bin/sh"), vec!["-c".to_string()]))
    }
}

//! Local filesystem and shell execution environment.

mod filesystem;
mod shell;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokio::fs;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::types::CreateDirOptions;
use crate::agent::harness::types::FileError;
use crate::agent::harness::types::FileErrorCode;
use crate::agent::harness::types::FileInfo;
use crate::agent::harness::types::FileKind;
use crate::agent::harness::types::FileSystem;
use crate::agent::harness::types::Result;
use crate::agent::harness::types::{err, ok};

/// Local filesystem execution environment for tests and local tooling.
pub struct LocalExecutionEnv {
    cwd: PathBuf,
    shell_path: Option<PathBuf>,
    shell_env: Option<HashMap<String, String>>,
    command_prefix: Option<String>,
}

impl LocalExecutionEnv {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            shell_path: None,
            shell_env: None,
            command_prefix: None,
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

    pub fn with_command_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.command_prefix = Some(prefix.into());
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
            Some(err(FileError::new(FileErrorCode::Aborted, "aborted", path.map(str::to_string))))
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
}

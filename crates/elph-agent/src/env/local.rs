//! Local filesystem and shell execution environment — ported from pi-agent `harness/env/nodejs.ts`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::harness::types::{
    CreateDirOptions, CreateTempFileOptions, ExecutionEnv, ExecutionError, ExecutionErrorCode, FileError,
    FileErrorCode, FileInfo, FileKind, FileSystem, ReadTextLinesOptions, RemoveOptions, Result, Shell,
    ShellExecOptions, ShellExecResult, err, ok,
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

#[async_trait]
impl FileSystem for LocalExecutionEnv {
    fn cwd(&self) -> &str {
        self.cwd
            .to_str()
            .expect("execution environment cwd must be valid UTF-8")
    }

    async fn absolute_path(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError> {
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, None) {
            return aborted;
        }
        ok(Self::normalize_path(&self.resolve_path(path)))
    }

    async fn join_path(&self, parts: &[&str], abort_token: Option<&CancellationToken>) -> Result<String, FileError> {
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, None) {
            return aborted;
        }
        let mut joined = PathBuf::new();
        for part in parts {
            joined.push(part);
        }
        ok(Self::normalize_path(&joined))
    }

    async fn read_text_file(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, Some(&normalized)) {
            return aborted;
        }
        match fs::read_to_string(&resolved).await {
            Ok(content) => {
                if let Some(aborted) = Self::abort_file_error::<String>(abort_token, Some(&normalized)) {
                    return aborted;
                }
                ok(content)
            }
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn read_text_lines(
        &self,
        path: &str,
        options: Option<ReadTextLinesOptions>,
    ) -> Result<Vec<String>, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        let options = options.unwrap_or(ReadTextLinesOptions {
            max_lines: None,
            abort_token: None,
        });
        if let Some(aborted) = Self::abort_file_error::<Vec<String>>(options.abort_token.as_ref(), Some(&normalized)) {
            return aborted;
        }
        if options.max_lines == Some(0) {
            return ok(Vec::new());
        }

        let file = match fs::File::open(&resolved).await {
            Ok(file) => file,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        };
        let mut reader = BufReader::new(file).lines();
        let mut lines = Vec::new();
        while let Some(line) = match reader.next_line().await {
            Ok(line) => line,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        } {
            if let Some(aborted) =
                Self::abort_file_error::<Vec<String>>(options.abort_token.as_ref(), Some(&normalized))
            {
                return aborted;
            }
            lines.push(line);
            if options.max_lines.is_some_and(|max| lines.len() >= max) {
                break;
            }
        }
        if let Some(aborted) = Self::abort_file_error::<Vec<String>>(options.abort_token.as_ref(), Some(&normalized)) {
            return aborted;
        }
        ok(lines)
    }

    async fn read_binary_file(
        &self,
        path: &str,
        abort_token: Option<&CancellationToken>,
    ) -> Result<Vec<u8>, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<Vec<u8>>(abort_token, Some(&normalized)) {
            return aborted;
        }
        match fs::read(&resolved).await {
            Ok(content) => {
                if let Some(aborted) = Self::abort_file_error::<Vec<u8>>(abort_token, Some(&normalized)) {
                    return aborted;
                }
                ok(content)
            }
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn write_file(
        &self,
        path: &str,
        content: &[u8],
        abort_token: Option<&CancellationToken>,
    ) -> Result<(), FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<()>(abort_token, Some(&normalized)) {
            return aborted;
        }
        if let Some(parent) = resolved.parent() {
            if let Err(error) = fs::create_dir_all(parent).await {
                return err(Self::to_file_error(error, Some(&Self::normalize_path(parent))));
            }
            if let Some(aborted) = Self::abort_file_error::<()>(abort_token, Some(&normalized)) {
                return aborted;
            }
        }
        match fs::write(&resolved, content).await {
            Ok(()) => ok(()),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn append_file(
        &self,
        path: &str,
        content: &[u8],
        abort_token: Option<&CancellationToken>,
    ) -> Result<(), FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<()>(abort_token, Some(&normalized)) {
            return aborted;
        }
        if let Some(parent) = resolved.parent()
            && let Err(error) = fs::create_dir_all(parent).await
        {
            return err(Self::to_file_error(error, Some(&Self::normalize_path(parent))));
        }
        use tokio::io::AsyncWriteExt;
        let mut file = match fs::OpenOptions::new().create(true).append(true).open(&resolved).await {
            Ok(file) => file,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        };
        match file.write_all(content).await {
            Ok(()) => ok(()),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn file_info(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<FileInfo, FileError> {
        if let Some(aborted) = Self::abort_file_error::<FileInfo>(abort_token, None) {
            return aborted;
        }
        let resolved = self.resolve_path(path);
        self.metadata_for_path(&resolved).await
    }

    async fn list_dir(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<Vec<FileInfo>, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<Vec<FileInfo>>(abort_token, Some(&normalized)) {
            return aborted;
        }
        let mut entries = match fs::read_dir(&resolved).await {
            Ok(entries) => entries,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        };

        let mut result = Vec::new();
        while let Some(entry) = match entries.next_entry().await {
            Ok(entry) => entry,
            Err(error) => return err(Self::to_file_error(error, Some(&normalized))),
        } {
            if let Some(aborted) = Self::abort_file_error::<Vec<FileInfo>>(abort_token, Some(&normalized)) {
                return aborted;
            }
            let entry_path = entry.path();
            match self.metadata_for_path(&entry_path).await {
                Result::Ok(info) => result.push(info),
                Result::Err(error) => return err(error),
            }
        }
        ok(result)
    }

    async fn canonical_path(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<String, FileError> {
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, Some(&normalized)) {
            return aborted;
        }
        match fs::canonicalize(&resolved).await {
            Ok(canonical) => ok(Self::normalize_path(&canonical)),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn exists(&self, path: &str, abort_token: Option<&CancellationToken>) -> Result<bool, FileError> {
        let info = self.file_info(path, abort_token).await;
        match info {
            Result::Ok(_) => ok(true),
            Result::Err(error) if error.code == FileErrorCode::NotFound => ok(false),
            Result::Err(error) => err(error),
        }
    }

    async fn create_dir(&self, path: &str, options: Option<CreateDirOptions>) -> Result<(), FileError> {
        let options = options.unwrap_or_default();
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<()>(options.abort_token.as_ref(), Some(&normalized)) {
            return aborted;
        }
        let result = if options.recursive {
            fs::create_dir_all(&resolved).await
        } else {
            fs::create_dir(&resolved).await
        };
        match result {
            Ok(()) => ok(()),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn remove(&self, path: &str, options: Option<RemoveOptions>) -> Result<(), FileError> {
        let options = options.unwrap_or_default();
        let resolved = self.resolve_path(path);
        let normalized = Self::normalize_path(&resolved);
        if let Some(aborted) = Self::abort_file_error::<()>(options.abort_token.as_ref(), Some(&normalized)) {
            return aborted;
        }

        let metadata = fs::symlink_metadata(&resolved).await;
        if metadata.is_err() {
            if options.force {
                return ok(());
            }
            return err(Self::to_file_error(
                metadata.expect_err("checked error"),
                Some(&normalized),
            ));
        }

        let metadata = metadata.expect("metadata exists");
        let result = if metadata.is_dir() {
            if options.recursive {
                fs::remove_dir_all(&resolved).await
            } else {
                fs::remove_dir(&resolved).await
            }
        } else {
            fs::remove_file(&resolved).await
        };
        match result {
            Ok(()) => ok(()),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn create_temp_dir(
        &self,
        prefix: &str,
        abort_token: Option<&CancellationToken>,
    ) -> Result<String, FileError> {
        if let Some(aborted) = Self::abort_file_error::<String>(abort_token, None) {
            return aborted;
        }
        let base = std::env::temp_dir();
        let path = base.join(format!("{prefix}{}", Uuid::new_v4()));
        match fs::create_dir_all(&path).await {
            Ok(()) => ok(Self::normalize_path(&path)),
            Err(error) => err(Self::to_file_error(error, None)),
        }
    }

    async fn create_temp_file(&self, options: Option<CreateTempFileOptions>) -> Result<String, FileError> {
        let options = options.unwrap_or_default();
        if let Some(aborted) = Self::abort_file_error::<String>(options.abort_token.as_ref(), None) {
            return aborted;
        }
        let dir = match self.create_temp_dir("tmp-", options.abort_token.as_ref()).await {
            Result::Ok(dir_path) => dir_path,
            Result::Err(error) => return err(error),
        };
        let dir_path = dir;
        let file_path =
            PathBuf::from(&dir_path).join(format!("{}{}{}", options.prefix, Uuid::new_v4(), options.suffix));
        let normalized = Self::normalize_path(&file_path);
        match fs::write(&file_path, &[] as &[u8]).await {
            Ok(()) => ok(normalized),
            Err(error) => err(Self::to_file_error(error, Some(&normalized))),
        }
    }

    async fn cleanup(&self) {}
}

#[async_trait]
impl Shell for LocalExecutionEnv {
    async fn exec(&self, command: &str, options: Option<ShellExecOptions>) -> Result<ShellExecResult, ExecutionError> {
        let options = options.unwrap_or(ShellExecOptions {
            cwd: None,
            env: None,
            timeout: None,
            abort_token: None,
            on_stdout: None,
            on_stderr: None,
        });

        if options.abort_token.as_ref().is_some_and(|t| t.is_cancelled()) {
            return err(ExecutionError::new(ExecutionErrorCode::Aborted, "aborted"));
        }

        let timeout_ms = match Self::resolve_timeout_ms(options.timeout) {
            Result::Ok(value) => value,
            Result::Err(error) => return err(error),
        };

        let cwd = options
            .cwd
            .as_deref()
            .map(|value| self.resolve_path(value))
            .unwrap_or_else(|| self.cwd.clone());

        let (shell, args) = match self.get_shell_config().await {
            Result::Ok(value) => value,
            Result::Err(error) => return err(error),
        };

        let mut cmd = Command::new(&shell);
        cmd.args(&args).arg(command);
        cmd.current_dir(&cwd);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        if let Some(shell_env) = &self.shell_env {
            cmd.envs(shell_env);
        }
        if let Some(extra_env) = &options.env {
            cmd.envs(extra_env);
        }

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) => {
                return err(ExecutionError::new(ExecutionErrorCode::SpawnError, error.to_string()));
            }
        };

        let wait_output = async move {
            match child.wait_with_output().await {
                Ok(output) => ok((
                    String::from_utf8_lossy(&output.stdout).into_owned(),
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                    output.status.code().unwrap_or(0),
                )),
                Err(error) => err(ExecutionError::new(ExecutionErrorCode::SpawnError, error.to_string())),
            }
        };

        let output = if let Some(token) = options.abort_token.clone() {
            tokio::select! {
                _ = token.cancelled() => {
                    return err(ExecutionError::new(ExecutionErrorCode::Aborted, "aborted"));
                }
                result = wait_output => result,
            }
        } else if let Some(timeout_ms) = timeout_ms {
            match tokio::time::timeout(Duration::from_millis(timeout_ms), wait_output).await {
                Ok(result) => result,
                Err(_) => {
                    return err(ExecutionError::new(
                        ExecutionErrorCode::Timeout,
                        format!("timeout:{}", options.timeout.unwrap_or_default()),
                    ));
                }
            }
        } else {
            wait_output.await
        };

        let (stdout, stderr, exit_code) = match output {
            Result::Ok(value) => value,
            Result::Err(error) => return err(error),
        };

        if let Some(on_stdout) = &options.on_stdout
            && !stdout.is_empty()
        {
            on_stdout(&stdout);
        }
        if let Some(on_stderr) = &options.on_stderr
            && !stderr.is_empty()
        {
            on_stderr(&stderr);
        }

        ok(ShellExecResult {
            stdout,
            stderr,
            exit_code,
        })
    }

    async fn cleanup(&self) {}
}

#[async_trait]
impl ExecutionEnv for LocalExecutionEnv {}

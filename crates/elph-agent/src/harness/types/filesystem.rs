//! Harness filesystem and shell types.

use std::future::Future;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::errors::{ExecutionError, FileError};
use super::result::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub mtime_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ReadTextLinesOptions {
    pub max_lines: Option<usize>,
    pub abort_token: Option<CancellationToken>,
}

#[derive(Debug, Clone)]
pub struct CreateDirOptions {
    pub recursive: bool,
    pub abort_token: Option<CancellationToken>,
}

impl Default for CreateDirOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            abort_token: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RemoveOptions {
    pub recursive: bool,
    pub force: bool,
    pub abort_token: Option<CancellationToken>,
}

#[derive(Debug, Clone, Default)]
pub struct CreateTempFileOptions {
    pub prefix: String,
    pub suffix: String,
    pub abort_token: Option<CancellationToken>,
}

/// Filesystem capability used by the harness.
pub trait FileSystem: Send + Sync {
    fn cwd(&self) -> &str;

    fn absolute_path<'a>(
        &'a self,
        path: &'a str,
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<String, FileError>> + Send + use<'a, Self>;
    fn join_path<'a>(
        &'a self,
        parts: &'a [&'a str],
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<String, FileError>> + Send + use<'a, Self>;
    fn read_text_file<'a>(
        &'a self,
        path: &'a str,
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<String, FileError>> + Send + use<'a, Self>;
    fn read_text_lines<'a>(
        &'a self,
        path: &'a str,
        options: Option<ReadTextLinesOptions>,
    ) -> impl Future<Output = Result<Vec<String>, FileError>> + Send + use<'a, Self>;
    fn read_binary_file<'a>(
        &'a self,
        path: &'a str,
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<Vec<u8>, FileError>> + Send + use<'a, Self>;
    fn write_file<'a>(
        &'a self,
        path: &'a str,
        content: &'a [u8],
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<(), FileError>> + Send + use<'a, Self>;
    fn append_file<'a>(
        &'a self,
        path: &'a str,
        content: &'a [u8],
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<(), FileError>> + Send + use<'a, Self>;
    fn file_info<'a>(
        &'a self,
        path: &'a str,
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<FileInfo, FileError>> + Send + use<'a, Self>;
    fn list_dir<'a>(
        &'a self,
        path: &'a str,
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<Vec<FileInfo>, FileError>> + Send + use<'a, Self>;
    fn canonical_path<'a>(
        &'a self,
        path: &'a str,
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<String, FileError>> + Send + use<'a, Self>;
    fn exists<'a>(
        &'a self,
        path: &'a str,
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<bool, FileError>> + Send + use<'a, Self>;
    fn create_dir<'a>(
        &'a self,
        path: &'a str,
        options: Option<CreateDirOptions>,
    ) -> impl Future<Output = Result<(), FileError>> + Send + use<'a, Self>;
    fn remove<'a>(
        &'a self,
        path: &'a str,
        options: Option<RemoveOptions>,
    ) -> impl Future<Output = Result<(), FileError>> + Send + use<'a, Self>;
    fn create_temp_dir<'a>(
        &'a self,
        prefix: &'a str,
        abort_token: Option<&'a CancellationToken>,
    ) -> impl Future<Output = Result<String, FileError>> + Send + use<'a, Self>;
    fn create_temp_file<'a>(
        &'a self,
        options: Option<CreateTempFileOptions>,
    ) -> impl Future<Output = Result<String, FileError>> + Send + use<'a, Self>;
    fn cleanup<'a>(&'a self) -> impl Future<Output = ()> + Send + use<'a, Self>;
}

#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub struct ShellExecOptions {
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout: Option<u64>,
    pub abort_token: Option<CancellationToken>,
    pub on_stdout: Option<Arc<dyn Fn(&str) + Send + Sync>>,
    pub on_stderr: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Shell execution capability used by the harness.
pub trait Shell: Send + Sync {
    fn exec<'a>(
        &'a self,
        command: &'a str,
        options: Option<ShellExecOptions>,
    ) -> impl Future<Output = Result<ShellExecResult, ExecutionError>> + Send + use<'a, Self>;
    fn cleanup<'a>(&'a self) -> impl Future<Output = ()> + Send + use<'a, Self>;
}

/// Filesystem and process execution environment used by the harness.
pub trait ExecutionEnv: FileSystem + Shell {}

//! Shared helpers for built-in coding tools.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use tokio_util::sync::CancellationToken;

use crate::harness::types::{ExecutionEnv, FileError, FileErrorCode, Result as HarnessResult};

pub fn check_aborted(signal: Option<&CancellationToken>) -> Result<()> {
    if signal.is_some_and(|token| token.is_cancelled()) {
        Err(anyhow!("Operation aborted"))
    } else {
        Ok(())
    }
}

pub async fn resolve_path(
    env: &Arc<dyn ExecutionEnv>,
    path: &str,
    signal: Option<&CancellationToken>,
) -> Result<String> {
    check_aborted(signal)?;
    match env.absolute_path(path, signal).await {
        HarnessResult::Ok(path) => Ok(path),
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}

pub fn file_error(error: FileError) -> anyhow::Error {
    anyhow!("{}", error.message)
}

pub async fn ensure_parent_dir(
    env: &Arc<dyn ExecutionEnv>,
    path: &str,
    signal: Option<&CancellationToken>,
) -> Result<()> {
    let parent = path.rsplit_once('/').map(|(parent, _)| parent).unwrap_or(".");
    if parent.is_empty() {
        return Ok(());
    }
    match env.exists(parent, signal).await {
        HarnessResult::Ok(true) => Ok(()),
        HarnessResult::Ok(false) => match env
            .create_dir(
                parent,
                Some(crate::harness::types::CreateDirOptions {
                    recursive: true,
                    abort_token: signal.cloned(),
                }),
            )
            .await
        {
            HarnessResult::Ok(()) => Ok(()),
            HarnessResult::Err(error) => Err(file_error(error)),
        },
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}

pub fn is_probably_image(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

pub async fn read_file_text(
    env: &Arc<dyn ExecutionEnv>,
    path: &str,
    signal: Option<&CancellationToken>,
) -> Result<String> {
    check_aborted(signal)?;
    match env.read_text_file(path, signal).await {
        HarnessResult::Ok(content) => Ok(content),
        HarnessResult::Err(error) if error.code == FileErrorCode::NotFound => Err(anyhow!("File not found: {path}")),
        HarnessResult::Err(error) => Err(file_error(error)),
    }
}

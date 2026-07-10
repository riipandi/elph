//! Shell output capture helpers — elph-agent module.

use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use crate::harness::types::{
    CreateTempFileOptions, ExecutionEnv, ExecutionError, ExecutionErrorCode, Result, ShellExecOptions, err, ok,
};
use crate::harness::utils::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_tail};

/// Result of capturing shell command output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCaptureResult {
    pub output: String,
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub truncated: bool,
    pub full_output_path: Option<String>,
}

/// Remove control characters and invalid Unicode from shell output.
pub fn sanitize_binary_output(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            let code = *ch as u32;
            if code == 0x09 || code == 0x0a || code == 0x0d {
                return true;
            }
            if code <= 0x1f {
                return false;
            }
            if (0xfff9..=0xfffb).contains(&code) {
                return false;
            }
            true
        })
        .collect()
}

/// Sanitize and truncate captured shell output from the tail.
pub fn finalize_shell_capture(output: &str, options: Option<TruncationOptions>) -> ShellCaptureResult {
    let sanitized = sanitize_binary_output(output).replace('\r', "");
    let truncation = truncate_tail(&sanitized, options.unwrap_or_default());
    ShellCaptureResult {
        output: truncation.content,
        exit_code: None,
        cancelled: false,
        truncated: truncation.truncated,
        full_output_path: None,
    }
}

pub fn to_execution_error(error: impl std::fmt::Display) -> ExecutionError {
    ExecutionError::new(ExecutionErrorCode::Unknown, error.to_string())
}

pub fn ok_shell_capture(result: ShellCaptureResult) -> Result<ShellCaptureResult, ExecutionError> {
    crate::harness::types::ok(result)
}

fn merge_shell_output(stdout: &str, stderr: &str) -> String {
    let mut combined = sanitize_binary_output(stdout).replace('\r', "");
    let stderr = sanitize_binary_output(stderr).replace('\r', "");
    if !stderr.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }
    combined
}

/// Options for shell capture — mirrors elph-agent.
#[derive(Debug, Clone, Default)]
pub struct ShellCaptureOptions {
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout: Option<u64>,
    pub abort_token: Option<CancellationToken>,
}

/// Execute a shell command and capture output with truncation and optional spill-to-disk.
pub async fn execute_shell_with_capture<E: ExecutionEnv>(
    env: &E,
    command: &str,
    options: Option<ShellCaptureOptions>,
) -> Result<ShellCaptureResult, ExecutionError> {
    let options = options.unwrap_or_default();
    let max_output_bytes = DEFAULT_MAX_BYTES * 2;
    let output_chunks: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let output_bytes: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let total_bytes: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let capture_error: Arc<Mutex<Option<ExecutionError>>> = Arc::new(Mutex::new(None));

    let on_chunk = {
        let output_chunks = output_chunks.clone();
        let output_bytes = output_bytes.clone();
        let total_bytes = total_bytes.clone();
        let capture_error = capture_error.clone();
        Arc::new(move |chunk: &str| {
            if capture_error.lock().expect("lock").is_some() {
                return;
            }
            let text = sanitize_binary_output(chunk).replace('\r', "");
            *total_bytes.lock().expect("lock") += text.len();
            let mut chunks = output_chunks.lock().expect("lock");
            chunks.push(text.clone());
            let mut bytes = output_bytes.lock().expect("lock");
            *bytes += text.len();
            while *bytes > max_output_bytes && chunks.len() > 1 {
                let removed = chunks.remove(0);
                *bytes -= removed.len();
            }
        })
    };

    let exec_result = env
        .exec(
            command,
            Some(ShellExecOptions {
                cwd: options.cwd,
                env: options.env,
                timeout: options.timeout,
                abort_token: options.abort_token.clone(),
                on_stdout: Some(on_chunk.clone()),
                on_stderr: Some(on_chunk),
            }),
        )
        .await;

    if let Some(error) = capture_error.lock().expect("lock").take() {
        return err(error);
    }

    let tail_output = output_chunks.lock().expect("lock").join("");
    let truncation = truncate_tail(&tail_output, TruncationOptions::default());
    let total = *total_bytes.lock().expect("lock");
    let spill_body = match &exec_result {
        Result::Ok(result) => merge_shell_output(&result.stdout, &result.stderr),
        Result::Err(_) => tail_output.clone(),
    };
    let mut full_output_path = None;

    if truncation.truncated || total > DEFAULT_MAX_BYTES {
        let temp_file = match env
            .create_temp_file(Some(CreateTempFileOptions {
                prefix: "bash-".to_string(),
                suffix: ".log".to_string(),
                abort_token: options.abort_token.clone(),
            }))
            .await
        {
            Result::Ok(path) => path,
            Result::Err(error) => return err(to_execution_error(error)),
        };
        match env
            .append_file(&temp_file, spill_body.as_bytes(), options.abort_token.as_ref())
            .await
        {
            Result::Ok(()) => full_output_path = Some(temp_file),
            Result::Err(error) => return err(to_execution_error(error)),
        }
    }

    let cancelled = options.abort_token.as_ref().is_some_and(|t| t.is_cancelled());

    match exec_result {
        Result::Ok(result) => ok(ShellCaptureResult {
            output: if truncation.truncated {
                truncation.content
            } else {
                tail_output
            },
            exit_code: if cancelled { None } else { Some(result.exit_code) },
            cancelled,
            truncated: truncation.truncated,
            full_output_path,
        }),
        Result::Err(error) => {
            if error.code == ExecutionErrorCode::Aborted || cancelled {
                return ok(ShellCaptureResult {
                    output: if truncation.truncated {
                        truncation.content
                    } else {
                        tail_output
                    },
                    exit_code: None,
                    cancelled: true,
                    truncated: truncation.truncated,
                    full_output_path,
                });
            }
            err(error)
        }
    }
}

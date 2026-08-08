//! Configurable local shell execution (PTY on Unix, piped fallback).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;

use super::error::{ExecError, ExecErrorCode, Result};
use super::output::sanitize_binary_output;
use super::process::{reap_bounded, terminate_child_tree};
use super::types::{ShellConfig, ShellExecOptions, ShellExecResult, ShellOutputCallback};

/// Max time we wait to reap a child after terminating its process group.
const REAP_GRACE: Duration = Duration::from_secs(2);
/// Max time we wait for a streaming/stdout wait to settle after a kill.
const SETTLE_GRACE: Duration = Duration::from_secs(1);

const MAX_TIMEOUT_MS: u64 = 2_147_483_647;
const MAX_TIMEOUT_SECONDS: u64 = MAX_TIMEOUT_MS / 1000;

/// Resolve the shell binary and invocation arguments from [`ShellConfig`].
pub async fn resolve_shell(config: &ShellConfig) -> Result<(PathBuf, Vec<String>)> {
    if let Some(shell_path) = &config.shell_path {
        if path_exists(shell_path).await {
            return Ok((shell_path.clone(), config.shell_args.clone()));
        }
        return Err(ExecError::new(
            ExecErrorCode::ShellUnavailable,
            format!("Custom shell path not found: {}", shell_path.display()),
        ));
    }

    let bash = PathBuf::from("/bin/bash");
    if path_exists(&bash).await {
        return Ok((bash, config.shell_args.clone()));
    }
    if let Some(path) = find_bash_on_path().await {
        return Ok((path, config.shell_args.clone()));
    }
    Ok((PathBuf::from("/bin/sh"), config.shell_args.clone()))
}

/// Execute `command` with the given [`ShellConfig`] and per-run options.
pub async fn exec_shell_command(
    config: &ShellConfig,
    command: &str,
    cwd: &Path,
    options: ShellExecOptions,
) -> Result<ShellExecResult> {
    if options.abort_token.as_ref().is_some_and(|t| t.is_cancelled()) {
        return Err(ExecError::aborted());
    }

    let timeout_ms = resolve_timeout_ms(options.timeout)?;

    let (shell, args) = resolve_shell(config).await?;

    #[cfg(unix)]
    if config.prefer_pty {
        let pty_size = options.pty_size.unwrap_or(config.pty_size);
        let request = PtyExecRequest {
            config,
            command,
            shell: &shell,
            shell_args: &args,
            cwd,
            options: &options,
            timeout_ms,
            pty_size,
        };
        match exec_pty(&request).await {
            Ok(result) => return Ok(result),
            Err(error) if error.code == ExecErrorCode::SpawnError => {
                log::debug!("PTY exec failed ({error}), falling back to piped shell");
            }
            Err(error) => return Err(error),
        }
    }

    exec_piped(config, command, &shell, &args, cwd, &options, timeout_ms).await
}

#[cfg(unix)]
struct PtyExecRequest<'a> {
    config: &'a ShellConfig,
    command: &'a str,
    shell: &'a Path,
    shell_args: &'a [String],
    cwd: &'a Path,
    options: &'a ShellExecOptions,
    timeout_ms: Option<u64>,
    pty_size: super::pty::PtySize,
}

#[cfg(unix)]
async fn exec_pty(request: &PtyExecRequest<'_>) -> Result<ShellExecResult> {
    use tokio::io::unix::AsyncFd;

    use super::pty::open_pty;

    let PtyExecRequest {
        config,
        command,
        shell,
        shell_args,
        cwd,
        options,
        timeout_ms,
        pty_size,
    } = request;

    let (pty_master, pts) = open_pty(*pty_size)?;
    pty_master.set_nonblocking()?;

    let (stdin, stdout, stderr) = pts.stdio_triple()?;
    let mut session_leader = pts.session_leader_pre_exec();

    let mut cmd = Command::new(shell);
    cmd.args(*shell_args).arg(command);
    cmd.current_dir(cwd);
    cmd.stdin(stdin);
    cmd.stdout(stdout);
    cmd.stderr(stderr);
    cmd.kill_on_drop(true);
    cmd.process_group(0);

    apply_env(&mut cmd, config, options);

    // SAFETY: The pre_exec closure runs in the child process after fork(),
    // before exec(). It calls session_leader() which is signal-safe and
    // only modifies child-local process state. No async/catching unwind.
    unsafe {
        cmd.pre_exec(move || {
            session_leader()?;
            Ok(())
        });
    }

    let mut child = cmd
        .spawn()
        .map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))?;

    let master_fd = pty_master.into_owned_fd();
    let async_pty =
        AsyncFd::new(master_fd).map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))?;
    let mut captured = String::new();
    let mut read_buf = vec![0u8; config.read_chunk_bytes.max(256)];
    let deadline = timeout_ms.map(|ms| tokio::time::Instant::now() + Duration::from_millis(ms));

    loop {
        if options.abort_token.as_ref().is_some_and(|t| t.is_cancelled()) {
            terminate_child_tree(&mut child, true).await;
            return kill_outcome(&mut child, &mut captured, true).await;
        }

        tokio::select! {
            _ = async {
                if let Some(token) = options.abort_token.clone() {
                    token.cancelled().await;
                } else {
                    std::future::pending().await
                }
            } => {
                terminate_child_tree(&mut child, true).await;
                return kill_outcome(&mut child, &mut captured, true).await;
            }
            _ = async {
                if let Some(deadline) = deadline {
                    tokio::time::sleep_until(deadline).await;
                } else {
                    std::future::pending().await
                }
            }, if deadline.is_some() => {
                terminate_child_tree(&mut child, false).await;
                return kill_outcome(&mut child, &mut captured, false).await;
            }
            readable = async_pty.readable() => {
                match readable {
                    Ok(mut guard) => {
                        let read_result = guard.try_io(|inner| {
                            rustix::io::read(inner, &mut read_buf).map_err(std::io::Error::from)
                        });
                        match read_result {
                            Ok(Ok(0)) => {
                                guard.clear_ready();
                            }
                            Ok(Ok(n)) => {
                                let chunk = String::from_utf8_lossy(&read_buf[..n]);
                                let chunk = sanitize_binary_output(&chunk);
                                if !chunk.is_empty() {
                                    captured.push_str(&chunk);
                                    if let Some(callback) = &options.on_stdout
                                        && let Some(error) = invoke_output_callback(callback, &chunk)
                                    {
                                        return Err(error);
                                    }
                                    if let Some(callback) = &options.on_stderr
                                        && let Some(error) = invoke_output_callback(callback, &chunk)
                                    {
                                        return Err(error);
                                    }
                                }
                                guard.clear_ready();
                            }
                            Ok(Err(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                guard.clear_ready();
                            }
                            Ok(Err(error)) => {
                                // A read error after we killed the process group is
                                // expected (PTY closed by death) — not a real failure.
                                if options.abort_token.as_ref().is_some_and(|t| t.is_cancelled()) {
                                    return kill_outcome(&mut child, &mut captured, true).await;
                                }
                                return Err(ExecError::new(ExecErrorCode::SpawnError, error.to_string()));
                            }
                            Err(_) => {
                                guard.clear_ready();
                            }
                        }
                    }
                    Err(error) => {
                        return Err(ExecError::new(ExecErrorCode::SpawnError, error.to_string()));
                    }
                }
            }
            status = child.wait() => {
                let status = status.map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))?;
                let exit_code = status.code().unwrap_or(0);
                return Ok(ShellExecResult {
                    stdout: captured,
                    stderr: String::new(),
                    exit_code,
                });
            }
        }
    }
}

async fn kill_outcome(
    child: &mut tokio::process::Child,
    _captured: &mut String,
    aborted: bool,
) -> Result<ShellExecResult> {
    // Reap with a bounded wait so a grandchild still holding the PTY cannot
    // hang us. We intentionally do not `.await` on `child.wait()` unbounded.
    let _ = reap_bounded(child, REAP_GRACE).await;
    if aborted {
        Err(ExecError::aborted())
    } else {
        Err(ExecError::new(ExecErrorCode::Timeout, format!("timeout:{}", 0)))
    }
}

async fn exec_piped(
    config: &ShellConfig,
    command: &str,
    shell: &Path,
    shell_args: &[String],
    cwd: &Path,
    options: &ShellExecOptions,
    timeout_ms: Option<u64>,
) -> Result<ShellExecResult> {
    let mut cmd = Command::new(shell);
    cmd.args(shell_args).arg(command);
    cmd.current_dir(cwd);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    apply_env(&mut cmd, config, options);

    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    let mut child = cmd
        .spawn()
        .map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))?;

    if options.on_stdout.is_some() || options.on_stderr.is_some() {
        return exec_piped_streaming(config, &mut child, options, timeout_ms).await;
    }

    let output = wait_child_with_output(child, options, timeout_ms).await?;
    let (stdout, stderr, exit_code) = output;

    Ok(ShellExecResult {
        stdout,
        stderr,
        exit_code,
    })
}

async fn wait_child_with_output(
    mut child: tokio::process::Child,
    options: &ShellExecOptions,
    timeout_ms: Option<u64>,
) -> Result<(String, String, i32)> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ExecError::new(ExecErrorCode::SpawnError, "stdout pipe unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ExecError::new(ExecErrorCode::SpawnError, "stderr pipe unavailable"))?;

    let stdout_task = tokio::spawn(async move { read_piped_stream(stdout, None, 8 * 1024).await });
    let stderr_task = tokio::spawn(async move { read_piped_stream(stderr, None, 8 * 1024).await });

    let status = await_child_status(&mut child, options, timeout_ms).await?;

    let stdout = stdout_task
        .await
        .map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))??;
    let stderr = stderr_task
        .await
        .map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))??;

    Ok((stdout, stderr, status.code().unwrap_or(0)))
}

/// Wait for a child's exit status with abort/timeout handling that terminates
/// the **whole process group** (not just the direct child) and reaps bounded —
/// so grandchildren holding the pipes can't hang the awaiters.
async fn await_child_status(
    child: &mut tokio::process::Child,
    options: &ShellExecOptions,
    timeout_ms: Option<u64>,
) -> Result<std::process::ExitStatus> {
    let deadline = timeout_ms.map(|ms| tokio::time::Instant::now() + Duration::from_millis(ms));

    tokio::select! {
        _ = async {
            if let Some(token) = options.abort_token.clone() {
                token.cancelled().await;
            } else {
                std::future::pending().await
            }
        } => {
            terminate_child_tree(child, true).await;
            let _ = reap_bounded(child, REAP_GRACE).await;
            Err(ExecError::aborted())
        }
        _ = async {
            if let Some(deadline) = deadline {
                tokio::time::sleep_until(deadline).await;
            } else {
                std::future::pending().await
            }
        }, if deadline.is_some() => {
            terminate_child_tree(child, false).await;
            let _ = reap_bounded(child, REAP_GRACE).await;
            Err(ExecError::new(
                ExecErrorCode::Timeout,
                format!("timeout:{}", options.timeout.unwrap_or_default()),
            ))
        }
        status = child.wait() => {
            status.map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))
        }
    }
}

async fn exec_piped_streaming(
    config: &ShellConfig,
    child: &mut tokio::process::Child,
    options: &ShellExecOptions,
    timeout_ms: Option<u64>,
) -> Result<ShellExecResult> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ExecError::new(ExecErrorCode::SpawnError, "stdout pipe unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ExecError::new(ExecErrorCode::SpawnError, "stderr pipe unavailable"))?;

    let chunk_bytes = config.read_chunk_bytes.max(256);
    let on_stdout = options.on_stdout.clone();
    let on_stderr = options.on_stderr.clone();

    let stdout_task = tokio::spawn(async move { read_piped_stream(stdout, on_stdout, chunk_bytes).await });
    let stderr_task = tokio::spawn(async move { read_piped_stream(stderr, on_stderr, chunk_bytes).await });

    let status = await_child_status(child, options, timeout_ms).await?;
    let stdout = stdout_task
        .await
        .map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))??;
    let stderr = stderr_task
        .await
        .map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))??;

    Ok(ShellExecResult {
        stdout,
        stderr,
        exit_code: status.code().unwrap_or(0),
    })
}

async fn read_piped_stream(
    mut reader: impl tokio::io::AsyncRead + Unpin + Send,
    callback: Option<ShellOutputCallback>,
    chunk_bytes: usize,
) -> Result<String> {
    use tokio::io::AsyncReadExt;

    let mut captured = String::new();
    let mut buf = vec![0u8; chunk_bytes];
    loop {
        let read = reader
            .read(&mut buf)
            .await
            .map_err(|error| ExecError::new(ExecErrorCode::SpawnError, error.to_string()))?;
        if read == 0 {
            break;
        }
        let chunk = sanitize_binary_output(&String::from_utf8_lossy(&buf[..read]));
        if chunk.is_empty() {
            continue;
        }
        captured.push_str(&chunk);
        if let Some(callback) = &callback
            && let Some(error) = invoke_output_callback(callback, &chunk)
        {
            return Err(error);
        }
    }
    Ok(captured)
}

fn apply_env(cmd: &mut Command, config: &ShellConfig, options: &ShellExecOptions) {
    if let Some(shell_env) = &config.base_env {
        cmd.envs(shell_env);
    }
    if let Some(extra_env) = &options.env {
        cmd.envs(extra_env);
    }
}

fn resolve_timeout_ms(timeout: Option<u64>) -> Result<Option<u64>> {
    let Some(timeout) = timeout else {
        return Ok(None);
    };
    if timeout == 0 {
        return Err(ExecError::new(
            ExecErrorCode::Timeout,
            "Invalid timeout: must be a finite number of seconds",
        ));
    }
    let timeout_ms = timeout.saturating_mul(1000);
    if timeout_ms > MAX_TIMEOUT_MS {
        return Err(ExecError::new(
            ExecErrorCode::Timeout,
            format!("Invalid timeout: maximum is {MAX_TIMEOUT_SECONDS} seconds"),
        ));
    }
    Ok(Some(timeout_ms))
}

async fn path_exists(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok()
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
    if path_exists(&path).await { Some(path) } else { None }
}

fn invoke_output_callback(callback: &ShellOutputCallback, chunk: &str) -> Option<ExecError> {
    let chunk = chunk.to_string();
    let callback = Arc::clone(callback);
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
            Some(ExecError::new(ExecErrorCode::CallbackError, message))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::Instant;
    use tokio_util::sync::CancellationToken;

    /// No streaming callbacks → goes through `exec_piped` → `wait_child_with_output`.
    #[tokio::test]
    async fn abort_kills_process_group_piped() {
        let config = ShellConfig::default().with_prefer_pty(false);
        let token = CancellationToken::new();

        let run = {
            let token = token.clone();
            tokio::spawn(async move {
                let result = exec_shell_command(
                    &config,
                    "sleep 30",
                    Path::new("/tmp"),
                    ShellExecOptions {
                        abort_token: Some(token),
                        ..Default::default()
                    },
                )
                .await;
                result
            })
        };

        // Let the command start, then abort.
        tokio::time::sleep(Duration::from_millis(300)).await;
        token.cancel();

        let started = Instant::now();
        let result = run.await.expect("join");
        let elapsed = started.elapsed();
        // Without process-group termination, `sleep 30` would keep the pipe open
        // and this test would hang ~30s. With the fix it must return well under 5s.
        assert!(
            elapsed < Duration::from_secs(5),
            "abort took {elapsed:?} — process group was not terminated"
        );
        let err = result.expect_err("expected aborted error");
        assert_eq!(err.code, ExecErrorCode::Aborted, "got {err}");
    }

    /// Assert the helper actually delivers with a grandchild that ignores SIGTERM.
    #[tokio::test]
    #[cfg(unix)]
    async fn abort_kills_grandchild_that_ignores_sigterm() {
        let config = ShellConfig::default().with_prefer_pty(false);
        let token = CancellationToken::new();
        let run = {
            let token = token.clone();
            tokio::spawn(async move {
                exec_shell_command(
                    &config,
                    "trap '' TERM; sleep 30",
                    Path::new("/tmp"),
                    ShellExecOptions {
                        abort_token: Some(token),
                        ..Default::default()
                    },
                )
                .await
            })
        };

        tokio::time::sleep(Duration::from_millis(300)).await;
        token.cancel();
        let started = Instant::now();
        let result = run.await.expect("join");
        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_secs(5), "SIGKILL escalation took {elapsed:?}");
        // Even with SIGTERM ignored, escalation to SIGKILL must abort.
        let err = result.expect_err("expected aborted error");
        assert_eq!(err.code, ExecErrorCode::Aborted, "got {err}");
    }

    /// PTY path: abort must also terminate the group quickly.
    #[tokio::test]
    #[cfg(unix)]
    async fn abort_kills_process_group_pty() {
        let config = ShellConfig::default().with_prefer_pty(true);
        let token = CancellationToken::new();
        let run = {
            let token = token.clone();
            tokio::spawn(async move {
                exec_shell_command(
                    &config,
                    "sleep 30",
                    Path::new("/tmp"),
                    ShellExecOptions {
                        abort_token: Some(token),
                        ..Default::default()
                    },
                )
                .await
            })
        };

        tokio::time::sleep(Duration::from_millis(300)).await;
        token.cancel();
        let started = Instant::now();
        let result = run.await.expect("join");
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "PTY abort took {elapsed:?} — process group was not terminated"
        );
        let err = result.expect_err("expected aborted error");
        assert_eq!(err.code, ExecErrorCode::Aborted, "got {err}");
    }

    /// Streaming path (callbacks set) must abort quickly too.
    #[tokio::test]
    async fn abort_streaming_kills_process_group() {
        let config = ShellConfig::default().with_prefer_pty(false);
        let token = CancellationToken::new();
        let saw_chunk = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let saw = saw_chunk.clone();
        let on_stdout: ShellOutputCallback = Arc::new(move |_chunk| {
            saw.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let run = {
            let token = token.clone();
            let on_stdout = on_stdout.clone();
            tokio::spawn(async move {
                exec_shell_command(
                    &config,
                    "echo start; sleep 30",
                    Path::new("/tmp"),
                    ShellExecOptions {
                        abort_token: Some(token),
                        on_stdout: Some(on_stdout),
                        on_stderr: None,
                        ..Default::default()
                    },
                )
                .await
            })
        };

        tokio::time::sleep(Duration::from_millis(300)).await;
        token.cancel();
        let started = Instant::now();
        let result = run.await.expect("join");
        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_secs(5), "streaming abort took {elapsed:?}");
        assert!(saw_chunk.load(std::sync::atomic::Ordering::SeqCst), "stream callback ran");
        let err = result.expect_err("expected aborted error");
        assert_eq!(err.code, ExecErrorCode::Aborted, "got {err}");
    }

    /// Timeout must terminate the process group too (not just the direct shell).
    #[tokio::test]
    async fn timeout_kills_process_group() {
        let config = ShellConfig::default().with_prefer_pty(false);
        let started = Instant::now();
        let result = exec_shell_command(
            &config,
            "sleep 30",
            Path::new("/tmp"),
            ShellExecOptions {
                timeout: Some(1),
                ..Default::default()
            },
        )
        .await;
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(6),
            "timeout took {elapsed:?} — process group was not terminated"
        );
        let err = result.expect_err("expected timeout error");
        assert_eq!(err.code, ExecErrorCode::Timeout, "got {err}");
    }
}

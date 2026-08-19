//! Integration tests for configurable shell execution.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use elph_agent::exec::{ShellConfig, ShellExecOptions, exec_shell_command};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn which_bash_for_test() -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("where.exe").arg("bash").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let path = std::path::PathBuf::from(line);
    path.exists().then_some(path)
}

#[tokio::test]
async fn executes_command_in_cwd_with_env_overrides() {
    let temp = TempDir::new().expect("temp dir");
    let config = ShellConfig::default();

    // Write a sentinel using the override env var. The file landing in `temp`
    // proves the command executed in the configured cwd — independent of how the
    // underlying shell represents paths (git-bash reports `$PWD` as MSYS-format
    // `/c/Users/...` on Windows), so this assertion is stable on every platform.
    let result = exec_shell_command(
        &config,
        "printf '%s' \"$NODE_ENV_TEST\" > __cwd_probe__",
        temp.path(),
        ShellExecOptions {
            env: Some([("NODE_ENV_TEST".to_string(), "ok".to_string())].into()),
            ..ShellExecOptions::default()
        },
    )
    .await
    .expect("exec");

    assert_eq!(result.exit_code, 0);
    let probe = temp.path().join("__cwd_probe__");
    let content = std::fs::read_to_string(&probe).expect("sentinel written in cwd");
    assert_eq!(content, "ok");
    std::fs::remove_file(&probe).ok();
}

#[tokio::test]
async fn streams_chunks_before_command_finishes() {
    let temp = TempDir::new().expect("temp dir");
    let seen = Arc::new(AtomicBool::new(false));
    let seen_capture = seen.clone();
    let config = ShellConfig::default();

    let result = exec_shell_command(
        &config,
        "printf early; sleep 0.2; printf late",
        temp.path(),
        ShellExecOptions {
            timeout: Some(5),
            on_stdout: Some(Arc::new(move |chunk| {
                if chunk.contains("early") {
                    seen_capture.store(true, Ordering::SeqCst);
                }
            })),
            ..ShellExecOptions::default()
        },
    )
    .await
    .expect("exec");

    assert!(seen.load(Ordering::SeqCst));
    assert!(result.stdout.contains("late"));
}

#[tokio::test]
async fn abort_token_cancels_long_running_command() {
    let temp = TempDir::new().expect("temp dir");
    let token = CancellationToken::new();
    let token_for_task = token.clone();
    let config = ShellConfig::default();

    let task = tokio::spawn(async move {
        exec_shell_command(
            &config,
            "sleep 30",
            temp.path(),
            ShellExecOptions {
                abort_token: Some(token_for_task),
                ..ShellExecOptions::default()
            },
        )
        .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    token.cancel();

    let result = task.await.expect("join").expect_err("should abort");
    assert_eq!(result.code, elph_agent::exec::ExecErrorCode::Aborted);
}

#[tokio::test]
async fn custom_shell_path_is_configurable() {
    let temp = TempDir::new().expect("temp dir");
    let sh = if cfg!(windows) {
        which_bash_for_test().expect("Git Bash required for this test on Windows")
    } else {
        std::path::PathBuf::from("/bin/sh")
    };
    let config = ShellConfig::default().with_shell_path(sh);

    let result = exec_shell_command(&config, "echo hi", temp.path(), ShellExecOptions::default())
        .await
        .expect("exec");

    assert!(result.stdout.contains("hi"));
}

#[tokio::test]
async fn streams_stdout_and_stderr_chunks() {
    let temp = TempDir::new().expect("temp dir");
    let stdout = Arc::new(Mutex::new(String::new()));
    let stderr = Arc::new(Mutex::new(String::new()));
    let stdout_capture = stdout.clone();
    let stderr_capture = stderr.clone();
    let config = ShellConfig::default().with_prefer_pty(false);

    let result = exec_shell_command(
        &config,
        "printf out; printf err 1>&2",
        temp.path(),
        ShellExecOptions {
            on_stdout: Some(Arc::new(move |chunk| {
                stdout_capture.lock().unwrap().push_str(chunk);
            })),
            on_stderr: Some(Arc::new(move |chunk| {
                stderr_capture.lock().unwrap().push_str(chunk);
            })),
            ..ShellExecOptions::default()
        },
    )
    .await
    .expect("exec");

    assert!(result.stdout.contains("out") || stdout.lock().unwrap().contains("out"));
    assert!(result.stderr.contains("err") || stderr.lock().unwrap().contains("err"));
}

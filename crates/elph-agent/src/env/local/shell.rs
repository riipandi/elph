//! Local shell execution implementation.

use super::LocalExecutionEnv;

use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::harness::types::{
    ExecutionEnv, ExecutionError, ExecutionErrorCode, Result, Shell, ShellExecOptions, ShellExecResult, err, ok,
};

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
            && let Some(error) = Self::invoke_output_callback(on_stdout, &stdout)
        {
            return err(error);
        }
        if let Some(on_stderr) = &options.on_stderr
            && !stderr.is_empty()
            && let Some(error) = Self::invoke_output_callback(on_stderr, &stderr)
        {
            return err(error);
        }

        ok(ShellExecResult {
            stdout,
            stderr,
            exit_code,
        })
    }

    async fn cleanup(&self) {}
}

impl ExecutionEnv for LocalExecutionEnv {}

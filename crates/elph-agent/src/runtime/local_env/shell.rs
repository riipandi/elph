//! Local shell execution — delegates to [`crate::exec`].

use std::path::Path;

use crate::exec::exec_shell_command;

use crate::agent::harness::types::ExecutionEnv;
use crate::agent::harness::types::ExecutionError;
use crate::agent::harness::types::Result;
use crate::agent::harness::types::Shell;
use crate::agent::harness::types::ShellExecOptions;
use crate::agent::harness::types::ShellExecResult;
use crate::agent::harness::types::{err, ok};

use super::LocalExecutionEnv;

impl Shell for LocalExecutionEnv {
    async fn exec(&self, command: &str, options: Option<ShellExecOptions>) -> Result<ShellExecResult, ExecutionError> {
        let options = options.unwrap_or_default();
        let cwd = options
            .cwd
            .as_deref()
            .map(|value| self.resolve_path(value))
            .unwrap_or_else(|| self.cwd.clone());

        match exec_shell_command(&self.shell_config(), command, cwd.as_path(), options).await {
            Ok(result) => ok(result),
            Err(error) => err(error),
        }
    }

    async fn cleanup(&self) {}
}

impl ExecutionEnv for LocalExecutionEnv {}

impl LocalExecutionEnv {
    pub fn shell_config(&self) -> crate::exec::ShellConfig {
        crate::exec::ShellConfig {
            shell_path: self.shell_path.clone(),
            shell_args: vec!["-c".to_string()],
            base_env: self.shell_env.clone(),
            ..crate::exec::ShellConfig::default()
        }
    }

    /// Run a shell command with an explicit working directory (for callers outside the harness).
    pub async fn exec_in(
        &self,
        command: &str,
        cwd: &Path,
        options: Option<ShellExecOptions>,
    ) -> Result<ShellExecResult, ExecutionError> {
        let options = options.unwrap_or_default();
        match exec_shell_command(&self.shell_config(), command, cwd, options).await {
            Ok(result) => ok(result),
            Err(error) => err(error),
        }
    }
}

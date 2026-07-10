use std::fs::File;
use std::io::{self, Write};

use clap::{Args, CommandFactory};
use clap_complete::{Shell, generate};

use super::Cli;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode};

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for (bash, elvish, fish, powershell, zsh)
    #[arg(short, long, value_name = "SHELL", value_enum, default_value_t = Shell::Bash)]
    pub shell: Shell,

    /// Write the script to a file instead of stdout
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<std::path::PathBuf>,
}

impl Default for CompletionsArgs {
    fn default() -> Self {
        Self {
            shell: Shell::from_env().unwrap_or(Shell::Bash),
            output: None,
        }
    }
}

pub fn handle(args: &CompletionsArgs) -> ExitCode {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();

    if let Some(path) = &args.output {
        let mut file = match File::create(path) {
            Ok(file) => file,
            Err(error) => {
                tracing::error!(path = %path.display(), %error, "failed to create completion file");
                return EXIT_ERROR;
            }
        };
        if let Err(error) = write_completions(args.shell, &mut cmd, &bin_name, &mut file) {
            tracing::error!(path = %path.display(), %error, "failed to write completion file");
            return EXIT_ERROR;
        }
        tracing::info!(path = %path.display(), shell = %args.shell, "wrote shell completions");
        return EXIT_SUCCESS;
    }

    if let Err(error) = write_completions(args.shell, &mut cmd, &bin_name, &mut io::stdout()) {
        tracing::error!(%error, "failed to write completions to stdout");
        return EXIT_ERROR;
    }
    EXIT_SUCCESS
}

fn write_completions(shell: Shell, cmd: &mut clap::Command, bin_name: &str, writer: &mut dyn Write) -> io::Result<()> {
    generate(shell, cmd, bin_name.to_string(), writer);
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_completion_includes_subcommands_and_ext_alias() {
        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();
        let mut script = Vec::new();
        generate(Shell::Bash, &mut cmd, bin_name, &mut script);

        let script = String::from_utf8(script).expect("utf8");
        assert!(script.contains("elph"), "expected bin name in script:\n{script}");
        assert!(
            script.contains("extensions"),
            "expected extensions subcommand:\n{script}"
        );
        assert!(script.contains("ext"), "expected ext alias:\n{script}");
        assert!(script.contains("memory"), "expected memory subcommand:\n{script}");
    }
}

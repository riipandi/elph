use std::env;

use clap::Args;

use crate::agent::{RunModeOptions, run_non_interactive};
use crate::cli::help;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths, Settings};

#[derive(Args, Default)]
pub struct RunArgs {
    /// Prompt to process non-interactively
    #[arg(value_name = "PROMPT")]
    pub prompt: Vec<String>,

    /// Model to use for this invocation (provider/model)
    #[arg(short = 'm', long = "model", value_name = "MODEL")]
    pub model: Option<String>,

    /// Output format
    #[arg(long = "output-format", value_name = "FORMAT", default_value = "text")]
    pub output_format: String,

    /// Continue the most recent session for the current working directory
    #[arg(short, long)]
    pub r#continue: bool,

    /// Resume a specific session by ID
    #[arg(short, long, value_name = "SESSION_ID")]
    pub session: Option<String>,

    /// Fork the session before continuing (requires --continue or --session)
    #[arg(long)]
    pub fork: bool,

    /// File(s) to attach to the prompt
    #[arg(short, long = "file", value_name = "FILE")]
    pub files: Vec<String>,

    /// Auto-approve tool executions
    #[arg(short, long)]
    pub brave: bool,
}

pub fn handle(args: &RunArgs) -> ExitCode {
    let prompt = args.prompt.join(" ");
    if prompt.trim().is_empty() {
        help::cli_error("run requires a prompt");
        return EXIT_ERROR;
    }

    let paths = match Paths::resolve() {
        Ok(p) => p,
        Err(err) => {
            help::cli_error(format!("resolve paths: {err}"));
            return EXIT_ERROR;
        }
    };
    let settings = match Settings::load(&paths) {
        Ok(s) => s,
        Err(err) => {
            help::cli_error(format!("load settings: {err}"));
            return EXIT_ERROR;
        }
    };
    let cwd = env::current_dir().unwrap_or_else(|_| ".".into());

    let resume_id = if args.r#continue { None } else { args.session.as_deref() };

    if args.fork {
        eprintln!("--fork is not yet implemented; continuing without fork");
    }
    if !args.files.is_empty() {
        eprintln!("file attachments not yet implemented: files={:?}", args.files);
    }
    if args.output_format != "text" {
        eprintln!("only text output-format is supported: format={}", args.output_format);
    }

    let result = elph_agent::block_on(run_non_interactive(RunModeOptions {
        paths: &paths,
        settings: &settings,
        cwd: &cwd,
        prompt: &prompt,
        model: args.model.as_deref(),
        resume_id,
        brave: args.brave,
    }));

    match result {
        Ok(()) => EXIT_SUCCESS,
        Err(err) => {
            help::cli_error(format!("run failed: {err}"));
            EXIT_ERROR
        }
    }
}

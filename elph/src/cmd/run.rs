use clap::Args;

use crate::runtime::{EXIT_SUCCESS, ExitCode};

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
    tracing::warn!(
        prompt = if prompt.is_empty() { "<none>" } else { prompt.as_str() },
        model = ?args.model,
        format = %args.output_format,
        continue_session = args.r#continue,
        session = ?args.session,
        fork = args.fork,
        files = ?args.files,
        brave = args.brave,
        "Run — not yet implemented"
    );
    EXIT_SUCCESS
}

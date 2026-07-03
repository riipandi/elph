use clap::Args;

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
    pub yolo: bool,
}

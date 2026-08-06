use std::env;

use clap::Args;

use crate::agent::RunModeOptions;
use crate::agent::run_non_interactive;
use crate::cli::help;
use crate::cli::session_launch::SessionLaunchMode;
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

    /// Continue the most recent session for the current project (CWD/PROJECT_DIR)
    #[arg(short = 'c', long = "continue")]
    pub r#continue: bool,

    /// Resume a specific session by session ID
    #[arg(short = 'r', long = "resume", value_name = "SESSION_ID", visible_alias = "session")]
    pub session: Option<String>,

    /// Fork the session before continuing (requires --continue or --resume)
    #[arg(long)]
    pub fork: bool,

    /// File(s) to attach to the prompt
    #[arg(short, long = "file", value_name = "FILE")]
    pub files: Vec<String>,

    /// Auto-approve tool executions
    #[arg(short, long)]
    pub brave: bool,

    /// Max retry attempts for provider API calls (default: 3)
    #[arg(long = "max-retries", value_name = "N")]
    pub max_retries: Option<u32>,

    /// Max backoff delay in milliseconds for retries (default: 30000)
    #[arg(long = "max-backoff-ms", value_name = "MS")]
    pub max_backoff_ms: Option<u64>,

    /// Circuit breaker failure threshold (default: 5)
    #[arg(long = "circuit-threshold", value_name = "N")]
    pub circuit_threshold: Option<u32>,

    /// Circuit breaker recovery timeout in milliseconds (default: 30000)
    #[arg(long = "circuit-timeout-ms", value_name = "MS")]
    pub circuit_timeout_ms: Option<u64>,
}

pub fn handle(args: &RunArgs) -> ExitCode {
    let prompt = args.prompt.join(" ");
    if prompt.trim().is_empty() {
        help::cli_error("run requires a prompt");
        return EXIT_ERROR;
    }

    // Initialize resilience manager with CLI overrides
    init_resilience_from_args(args);

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
    let project_dir = paths.project_dir().clone();
    let cwd = env::current_dir().unwrap_or_else(|_| project_dir.clone());

    let mode = match SessionLaunchMode::from_flags(args.r#continue, args.session.clone()) {
        Ok(m) => m,
        Err(err) => {
            help::cli_error(err);
            return EXIT_ERROR;
        }
    };
    let resume_id = match elph_agent::block_on(mode.resolve_resume_id(&paths, &project_dir)) {
        Ok(id) => id,
        Err(err) => {
            help::cli_error(err);
            return EXIT_ERROR;
        }
    };

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
        resume_id: resume_id.as_deref(),
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

/// Initialize the global resilience manager from CLI arguments.
fn init_resilience_from_args(args: &RunArgs) {
    use elph_ai::resilience::{ResilienceConfig, ResilienceManager, init_global_manager};
    use std::time::Duration;

    let mut config = ResilienceConfig::default();

    if let Some(threshold) = args.circuit_threshold {
        config = config.with_failure_threshold(threshold);
    }
    if let Some(timeout_ms) = args.circuit_timeout_ms {
        config = config.with_recovery_timeout(Duration::from_millis(timeout_ms));
    }
    if let Some(retries) = args.max_retries {
        config = config.with_max_retries(retries);
    }
    if let Some(max_backoff_ms) = args.max_backoff_ms {
        config = config.with_backoff(Duration::from_millis(500), Duration::from_millis(max_backoff_ms));
    }

    // Only initialize if any override was provided
    if args.circuit_threshold.is_some()
        || args.circuit_timeout_ms.is_some()
        || args.max_retries.is_some()
        || args.max_backoff_ms.is_some()
    {
        init_global_manager(ResilienceManager::new(config));
    }
}

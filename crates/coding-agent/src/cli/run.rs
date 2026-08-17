use std::env;
use std::path::PathBuf;

use clap::Args;

use crate::agent::{
    OutputFormat, RunModeOptions, parse_agent_mode, parse_effort, resolve_system_prompt_arg, run_non_interactive,
};
use crate::cli::help;
use crate::cli::session_launch::SessionLaunchMode;
use crate::platform::{EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths, Settings};
use crate::types::AgentMode;

#[derive(Args, Default)]
pub struct RunArgs {
    /// Prompt to process non-interactively (joined with spaces when multiple)
    #[arg(value_name = "PROMPT")]
    pub prompt: Vec<String>,

    /// Load the prompt from a file (UTF-8)
    #[arg(long = "prompt-file", value_name = "PATH")]
    pub prompt_file: Option<PathBuf>,

    /// Model to use for this invocation (`provider/model_id`)
    #[arg(short = 'm', long = "model", value_name = "MODEL")]
    pub model: Option<String>,

    /// Agent mode (tool policy). Default for headless: **brave**
    #[arg(long = "mode", value_name = "MODE", default_value = "brave")]
    pub mode: String,

    /// Alias for `--mode=brave` (auto-approve tools)
    #[arg(short, long)]
    pub brave: bool,

    /// Override the system prompt (literal text, `@path`, or a file path)
    #[arg(long = "system-prompt", value_name = "TEXT")]
    pub system_prompt: Option<String>,

    /// Do not keep a durable session (delete after the run)
    #[arg(long = "no-session")]
    pub no_session: bool,

    /// Working directory (project root for tools + session store)
    #[arg(long = "cwd", value_name = "PATH")]
    pub cwd: Option<PathBuf>,

    /// Abort after this many tool invocations (agent tool rounds)
    #[arg(long = "max-turns", value_name = "N")]
    pub max_turns: Option<u32>,

    /// Output format: plain | pretty | json | stream-json | stream-message-json
    ///
    /// `pretty` renders CommonMark/markdown to the terminal (rendown; crossterm width).
    /// Aliases: `--output` (same values). `markdown` / `md` map to `pretty`.
    #[arg(
        long = "output-format",
        visible_alias = "output",
        value_name = "FORMAT",
        default_value = "plain"
    )]
    pub output_format: String,

    /// Reasoning / thinking effort: off | low | medium | high | xhigh | max
    #[arg(long = "effort", value_name = "LEVEL", visible_alias = "reasoning-effort")]
    pub effort: Option<String>,

    /// Open this session id, or create it if missing
    #[arg(long = "session-id", value_name = "ID")]
    pub session_id: Option<String>,

    /// Session display name
    #[arg(short = 'n', long = "name", value_name = "NAME")]
    pub name: Option<String>,

    /// Continue the most recent session for the current project (CWD/PROJECT_DIR)
    #[arg(short = 'c', long = "continue")]
    pub r#continue: bool,

    /// Resume a specific session by session ID (must already exist)
    #[arg(short = 'r', long = "resume", value_name = "SESSION_ID", visible_alias = "session")]
    pub session: Option<String>,

    /// Fork the session before continuing (requires --continue or --resume)
    #[arg(long)]
    pub fork: bool,

    /// File(s) to attach to the prompt
    #[arg(short, long = "file", value_name = "FILE")]
    pub files: Vec<String>,

    /// Max retry attempts for provider API calls (default: 5)
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
    // --cwd first so Paths::resolve binds to the target project tree.
    if let Some(cwd) = &args.cwd
        && let Err(err) = env::set_current_dir(cwd)
    {
        help::cli_error(format!("--cwd {}: {err}", cwd.display()));
        return EXIT_ERROR;
    }

    let prompt = match resolve_prompt(args) {
        Ok(p) => p,
        Err(err) => {
            help::cli_error(err);
            return EXIT_ERROR;
        }
    };

    let output_format = match OutputFormat::parse(&args.output_format) {
        Ok(f) => f,
        Err(err) => {
            help::cli_error(err);
            return EXIT_ERROR;
        }
    };

    let mode = match resolve_mode(args) {
        Ok(m) => m,
        Err(err) => {
            help::cli_error(err);
            return EXIT_ERROR;
        }
    };

    let effort = match args.effort.as_deref() {
        Some(raw) => match parse_effort(raw) {
            Ok(e) => Some(e),
            Err(err) => {
                help::cli_error(err);
                return EXIT_ERROR;
            }
        },
        None => None,
    };

    let system_prompt_override = match args.system_prompt.as_deref() {
        Some(raw) => match resolve_system_prompt_arg(raw) {
            Ok(s) => Some(s),
            Err(err) => {
                help::cli_error(err);
                return EXIT_ERROR;
            }
        },
        None => None,
    };

    if let Some(n) = args.max_turns
        && n == 0
    {
        help::cli_error("--max-turns must be >= 1");
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

    let mode_launch = match SessionLaunchMode::from_run_flags(
        args.r#continue,
        args.session.clone(),
        args.session_id.clone(),
        args.no_session,
    ) {
        Ok(m) => m,
        Err(err) => {
            help::cli_error(err);
            return EXIT_ERROR;
        }
    };

    let (resume_id, create_if_missing) = match elph_agent::block_on(mode_launch.resolve(&paths, &project_dir)) {
        Ok(v) => v,
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

    let system_prompt_ref = system_prompt_override.as_deref();
    // Binary is `#[tokio::main]` (already multi-thread). Never nest Runtime::block_on —
    // use elph_agent::block_on which does block_in_place + Handle::block_on on the
    // existing runtime (spawned tasks + wait-line OS thread stay concurrent).
    let result = elph_agent::block_on(run_non_interactive(RunModeOptions {
        paths: &paths,
        settings: &settings,
        cwd: &cwd,
        prompt: &prompt,
        model: args.model.as_deref(),
        resume_id: resume_id.as_deref(),
        create_if_missing,
        mode,
        system_prompt_override: system_prompt_ref,
        no_session: args.no_session,
        max_turns: args.max_turns,
        output_format,
        effort,
        name: args.name.as_deref(),
    }));

    match result {
        Ok(_) => EXIT_SUCCESS,
        Err(err) => {
            help::cli_error(format!("run failed: {err}"));
            EXIT_ERROR
        }
    }
}

fn resolve_prompt(args: &RunArgs) -> Result<String, String> {
    if let Some(path) = &args.prompt_file {
        let text = std::fs::read_to_string(path).map_err(|e| format!("read --prompt-file {}: {e}", path.display()))?;
        if text.trim().is_empty() {
            return Err("--prompt-file is empty".into());
        }
        return Ok(text);
    }
    let prompt = args.prompt.join(" ");
    if prompt.trim().is_empty() {
        return Err("run requires a prompt (positional) or --prompt-file".into());
    }
    Ok(prompt)
}

fn resolve_mode(args: &RunArgs) -> Result<AgentMode, String> {
    let mode = parse_agent_mode(&args.mode).map_err(|e| e.to_string())?;
    // `--brave` is an alias for brave mode. Conflict only when `--mode` is a different mode.
    if args.brave && mode != AgentMode::Brave {
        return Err("cannot use --brave with --mode (pick one)".into());
    }
    if args.brave {
        return Ok(AgentMode::Brave);
    }
    Ok(mode)
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

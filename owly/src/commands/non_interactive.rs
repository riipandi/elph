use anyhow::Result;

use crate::agent::{self, RunAgentResult};
use crate::cli::{print_chat_header, print_command_header, print_completion};
use crate::config::Config;
use crate::docs::{self, DocumentationSnapshot};
use crate::metadata;
use crate::mode::{RunMode, WikiContext};
use crate::session::SessionStore;

use super::Command;
use super::doc_run::{apply_doc_run_result, run_init_agent, run_update_agent, should_skip_update_noop};

/// Print a plan for init/update/chat without calling the LLM or writing wiki pages.
pub(super) fn run_dry_run(config: &Config, ctx: &WikiContext, command: &Command) -> Result<()> {
    let wiki_root = ctx.wiki_root();
    println!("Owly dry-run (no LLM, no wiki writes)");
    println!("  mode:     {}", ctx.mode.as_str());
    println!("  provider: {}", config.provider);
    println!("  model:    {}", config.model_id);
    println!("  wiki:     {}", wiki_root.display());
    match command {
        Command::Init => {
            if wiki_root.exists() && docs::create_snapshot(ctx)?.exists {
                println!("  action:   init → would delegate to update (wiki already exists)");
            } else {
                println!("  action:   init → would create wiki via elph-agent");
            }
            if ctx.mode == RunMode::Code {
                println!(
                    "  after:    refresh AGENTS.md/CLAUDE.md (OWLY:START/END) + optional .github/workflows/owly-update.yml"
                );
            }
        }
        Command::Update => {
            if !wiki_root.exists() || !docs::create_snapshot(ctx)?.exists {
                println!("  action:   update → would init first (wiki missing)");
            } else if metadata::is_update_noop_ctx(ctx) {
                println!("  action:   update → would skip (no-op: no relevant changes since last update)");
            } else {
                println!("  action:   update → would run surgical doc refresh via elph-agent");
            }
            if ctx.mode == RunMode::Code {
                println!("  after:    update .last-update.json only if wiki content changes; sync agent guidance");
            } else {
                println!("  after:    update ~/.owly/wiki/.last-update.json only if wiki content changes");
            }
        }
        Command::Chat { message } => {
            let preview = message.as_deref().unwrap_or("(interactive)");
            println!("  action:   chat → would send message via elph-agent");
            println!("  message:  {preview}");
        }
    }
    Ok(())
}

pub(super) async fn run_non_interactive(
    config: &Config,
    ctx: &WikiContext,
    command: Command,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    match command {
        Command::Init => run_non_interactive_init(config, ctx, print_mode, stream, verbose).await,
        Command::Update => run_non_interactive_update(config, ctx, print_mode, stream, verbose).await,
        Command::Chat { message: Some(msg) } => {
            let session_anchor = ctx.agent_cwd();
            let mut session = SessionStore::open(&session_anchor).await?;
            run_non_interactive_chat(config, ctx, &msg, print_mode, stream, verbose, &mut session).await
        }
        Command::Chat { message: None } => {
            anyhow::bail!("Pass a message, --init, or --update.");
        }
    }
}

async fn run_non_interactive_init(
    config: &Config,
    ctx: &WikiContext,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    if docs::create_snapshot(ctx)?.exists {
        println!("Documentation already exists. Updating...");
        println!();
        return do_non_interactive_update(config, ctx, print_mode, stream, verbose).await;
    }
    do_non_interactive_init(config, ctx, print_mode, stream, verbose).await
}

async fn run_non_interactive_update(
    config: &Config,
    ctx: &WikiContext,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    if !docs::create_snapshot(ctx)?.exists {
        println!("No documentation found. Initializing...");
        println!();
        return do_non_interactive_init(config, ctx, print_mode, stream, verbose).await;
    }
    do_non_interactive_update(config, ctx, print_mode, stream, verbose).await
}

async fn do_non_interactive_init(
    config: &Config,
    ctx: &WikiContext,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    print_command_header("Init", &config.provider, &config.model_id);

    let (result, snapshot) = run_init_agent(config, ctx, None, print_mode, stream, verbose).await?;

    finish_non_interactive_doc_run(ctx, config, "init", &result, &snapshot, print_mode)
}

async fn do_non_interactive_update(
    config: &Config,
    ctx: &WikiContext,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<()> {
    if should_skip_update_noop(ctx, None, stream, verbose, print_mode) {
        println!("No changes detected. Skipping.");
        return Ok(());
    }

    print_command_header("Update", &config.provider, &config.model_id);

    let (result, snapshot) = run_update_agent(config, ctx, None, print_mode, stream, verbose).await?;

    finish_non_interactive_doc_run(ctx, config, "update", &result, &snapshot, print_mode)
}

async fn run_non_interactive_chat(
    config: &Config,
    ctx: &WikiContext,
    message: &str,
    print_mode: bool,
    stream: bool,
    verbose: bool,
    session: &mut SessionStore,
) -> Result<()> {
    print_chat_header(&config.provider, &config.model_id);

    let (system_prompt, user_prompt) = agent::prepare_chat_command(ctx, message);
    let user_prompt = format!("{user_prompt}{}", crate::prompts::create_runtime_note(ctx));

    let result = agent::run_agent(agent::RunAgentOptions {
        command: "chat",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        ctx,
        print_mode,
        stream,
        verbose,
        session: Some(session),
        is_followup: false,
        docs_snapshot_before: None,
    })
    .await?;

    if !result.completion_message.is_empty() {
        print!("{}", result.completion_message);
        if !result.completion_message.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

fn finish_non_interactive_doc_run(
    ctx: &WikiContext,
    config: &Config,
    command: &str,
    result: &RunAgentResult,
    before: &DocumentationSnapshot,
    print_mode: bool,
) -> Result<()> {
    if result.skipped {
        if !print_mode {
            println!("{}", result.completion_message);
        }
        return Ok(());
    }

    apply_doc_run_result(ctx, config, command, result, before)?;

    if print_mode {
        if !result.completion_message.is_empty() {
            println!("{}", result.completion_message);
        }
    } else {
        print_completion(&result.completion_message);
    }

    Ok(())
}

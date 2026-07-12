//! Shared init/update agent runs for code and personal modes (terminal).

use anyhow::{Context, Result};

use crate::agent::{self, RunAgentResult};
use crate::config::Config;
use crate::docs::{self, DocumentationSnapshot};
use crate::ecosystem;
use crate::instructions;
use crate::metadata;
use crate::mode::{RunMode, WikiContext};

/// Whether an update run should be skipped before invoking the agent.
pub fn should_skip_update_noop(
    ctx: &WikiContext,
    user_message: Option<&str>,
    stream: bool,
    verbose: bool,
    print_mode: bool,
) -> bool {
    user_message.is_none() && !stream && !verbose && !print_mode && metadata::is_update_noop_ctx(ctx)
}

pub async fn run_init_agent(
    config: &Config,
    ctx: &WikiContext,
    user_message: Option<&str>,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<(RunAgentResult, DocumentationSnapshot)> {
    let ctx_for_prompt = ctx.clone();
    tokio::task::spawn_blocking(move || instructions::prompt_wiki_brief_if_missing(&ctx_for_prompt))
        .await
        .context("wiki brief prompt interrupted")??;

    let snapshot = docs::create_snapshot(ctx)?;
    let (system_prompt, user_prompt) = agent::prepare_init_command(ctx, user_message, &config.model_id);
    let user_prompt = format!("{user_prompt}{}", crate::prompts::create_runtime_note(ctx));

    let result = agent::run_agent(agent::RunAgentOptions {
        command: "init",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        ctx,
        print_mode,
        stream,
        verbose,
        session: None,
        is_followup: false,
        docs_snapshot_before: Some(snapshot.clone()),
    })
    .await?;

    Ok((result, snapshot))
}

pub async fn run_update_agent(
    config: &Config,
    ctx: &WikiContext,
    user_message: Option<&str>,
    print_mode: bool,
    stream: bool,
    verbose: bool,
) -> Result<(RunAgentResult, DocumentationSnapshot)> {
    let snapshot = docs::create_snapshot(ctx)?;
    let last_update = metadata::load_metadata_ctx(ctx);
    let (system_prompt, user_prompt) =
        agent::prepare_update_command(ctx, user_message, &config.model_id, last_update.as_ref());
    let user_prompt = format!("{user_prompt}{}", crate::prompts::create_runtime_note(ctx));

    let result = agent::run_agent(agent::RunAgentOptions {
        command: "update",
        system_prompt: &system_prompt,
        user_prompt: &user_prompt,
        config,
        ctx,
        print_mode,
        stream,
        verbose,
        session: None,
        is_followup: false,
        docs_snapshot_before: Some(snapshot.clone()),
    })
    .await?;

    Ok((result, snapshot))
}

pub fn apply_doc_run_result(
    ctx: &WikiContext,
    config: &Config,
    command: &str,
    result: &RunAgentResult,
    before: &DocumentationSnapshot,
) -> Result<()> {
    if result.skipped {
        return Ok(());
    }
    if result.docs_changed {
        docs::save_update_metadata_if_changed(ctx, command, &config.elph_model_id(), before)?;
        if ctx.mode == RunMode::Code {
            ecosystem::ensure_code_mode_repo_setup(&ctx.repo_cwd)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::WikiContext;

    #[test]
    fn stream_or_verbose_bypasses_code_mode_noop() {
        let ctx = WikiContext::code("/tmp/repo");
        assert!(!should_skip_update_noop(&ctx, None, true, false, false));
        assert!(!should_skip_update_noop(&ctx, None, false, true, false));
        assert!(!should_skip_update_noop(&ctx, None, false, false, true));
    }

    #[test]
    fn personal_mode_never_noops_without_message() {
        let ctx = WikiContext::personal("/tmp/any");
        assert!(!should_skip_update_noop(&ctx, None, false, false, false));
    }
}

use anyhow::Result;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use elph_agent::{
    Agent, AgentOptions, AgentState, LocalExecutionEnv, PartialAgentState, create_all_tools, create_read_only_tools,
};

use crate::ask_user::{AskUserBridge, create_ask_tools};
use crate::cli::format_stream_footer;
use crate::config::Config;
use crate::docs::{self, DocumentationSnapshot};
use crate::env;
use crate::mode::WikiContext;
use crate::session::SessionStore;

use crate::interactive;

use super::listeners::{create_checkpoint_write_subscriber, create_event_subscriber};
use super::model::resolve_model_and_auth;

/// Result of a single agent invocation.
#[derive(Debug)]
pub struct RunAgentResult {
    pub completion_message: String,
    pub docs_changed: bool,
    pub skipped: bool,
}

/// Options for running the agent
pub struct RunAgentOptions<'a> {
    pub command: &'a str,
    pub system_prompt: &'a str,
    pub user_prompt: &'a str,
    pub config: &'a Config,
    pub ctx: &'a WikiContext,
    pub print_mode: bool,
    pub stream: bool,
    pub verbose: bool,
    pub session: Option<&'a mut SessionStore>,
    pub is_followup: bool,
    pub docs_snapshot_before: Option<DocumentationSnapshot>,
}

fn print_assistant_response(state: &AgentState) -> bool {
    let Some(elph_ai::Message::Assistant(assistant)) = state.messages.last().and_then(|m| m.as_llm()) else {
        return false;
    };
    let mut wrote = false;
    for block in &assistant.content {
        if let elph_ai::AssistantContentBlock::Text(t) = block
            && !t.text.is_empty()
        {
            print!("{}", t.text);
            wrote = true;
        }
    }
    if wrote {
        println!();
        let _ = std::io::stdout().flush();
    }
    wrote
}

/// Run the agent with the given command.
pub async fn run_agent(opts: RunAgentOptions<'_>) -> Result<RunAgentResult> {
    let RunAgentOptions {
        command,
        system_prompt,
        user_prompt,
        config,
        ctx,
        print_mode,
        stream,
        verbose,
        mut session,
        is_followup,
        docs_snapshot_before,
    } = opts;

    env::debug_log(format!("command={command} followup={is_followup}"));
    let start_time = Instant::now();

    let (model, models_arc, stream_fn) = resolve_model_and_auth(config).await?;
    let agent_cwd = ctx.agent_cwd();
    let env = Arc::new(LocalExecutionEnv::new(&agent_cwd));

    let (mut agent_tools, base_tool_str) = if command == "chat" {
        (create_read_only_tools(env.clone()), "read, grep, find, ls (read-only mode)")
    } else {
        (create_all_tools(env.clone()), "read, bash, edit, write, grep, find, ls")
    };

    if command == "chat" {
        let ask_bridge = AskUserBridge::new();
        agent_tools.extend(create_ask_tools(ask_bridge));
    }

    let tool_names_str = if command == "chat" {
        format!("{base_tool_str}, ask_text, ask_select, ask_confirm")
    } else {
        base_tool_str.to_string()
    };
    let full_system_prompt = format!("{system_prompt}\n\nAvailable tools for this session: {tool_names_str}");

    let turn_write_ctx = if let Some(session) = session.as_mut() {
        session.ensure_bootstrap_checkpoint().await?;
        Some(session.turn_write_context())
    } else {
        None
    };

    let restored_messages = if let Some(session) = session.as_ref() {
        if is_followup || command == "chat" {
            session.load_messages().await?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let session_id = session.as_ref().map(|s| s.thread_id().to_string());

    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(full_system_prompt),
            model: Some(model.clone()),
            tools: Some(agent_tools),
            messages: if restored_messages.is_empty() {
                None
            } else {
                Some(restored_messages)
            },
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        session_id,
        ..Default::default()
    });

    let generating = interactive::progress_spinner("Thinking...");
    let saw_any_delta = Arc::new(AtomicBool::new(false));
    let stream_ends_with_newline = Arc::new(AtomicBool::new(true));

    if let Some(write_ctx) = turn_write_ctx {
        agent.subscribe(create_checkpoint_write_subscriber(write_ctx)).await;
    }

    agent
        .subscribe(create_event_subscriber(
            stream,
            verbose,
            generating.clone(),
            saw_any_delta.clone(),
            stream_ends_with_newline.clone(),
        ))
        .await;

    agent.prompt_text(user_prompt.to_string(), None).await?;
    agent.wait_for_idle().await;

    let elapsed = start_time.elapsed();
    let state = agent.state().await;

    // Init/update runs use ephemeral checkpoints: only chat persists Turso state.
    if let Some(session) = session
        && command == "chat"
    {
        session.save_messages(&state.messages, command).await?;
        if let Err(err) = session
            .try_auto_name(&state.messages, &model, models_arc.as_ref())
            .await
        {
            tracing::warn!(error = %err, "auto session naming failed");
        }
    }

    let docs_changed = if let Some(before) = docs_snapshot_before.as_ref() {
        let after = docs::create_snapshot(ctx)?;
        docs::has_changed(before, &after)
    } else {
        false
    };

    let streamed = stream && saw_any_delta.load(Ordering::SeqCst);
    let ends_with_newline = stream_ends_with_newline.load(Ordering::SeqCst);
    let completion_message = if print_mode && !stream {
        print_assistant_response(&state);
        String::new()
    } else if !stream || !streamed {
        print_assistant_response(&state);
        format_stream_footer(elapsed.as_secs_f64(), false, true)
    } else {
        format_stream_footer(elapsed.as_secs_f64(), true, ends_with_newline)
    };

    Ok(RunAgentResult {
        completion_message,
        docs_changed,
        skipped: false,
    })
}

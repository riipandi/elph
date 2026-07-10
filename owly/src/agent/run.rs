use anyhow::Result;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use tokio::sync::mpsc;

use elph_agent::{Agent, AgentOptions, LocalExecutionEnv, PartialAgentState, create_all_tools, create_read_only_tools};

use crate::ask_user::{create_ask_confirm_tool, create_ask_select_tool, create_ask_text_tool};
use crate::config::Config;
use crate::docs::{self, DocumentationSnapshot};
use crate::env;
use crate::session::SessionStore;
use crate::ui_events::AgentUiEvent;

use super::listeners::{create_checkpoint_write_subscriber, create_event_subscriber, create_tui_event_subscriber};
use super::model::{progress_spinner, resolve_model_and_auth};

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
    pub cwd: &'a Path,
    pub print_mode: bool,
    pub stream: bool,
    pub verbose: bool,
    pub session: Option<&'a mut SessionStore>,
    pub is_followup: bool,
    pub docs_snapshot_before: Option<DocumentationSnapshot>,
    /// Suppress spinners and direct stdout/stderr writes (interactive TUI mode).
    pub quiet: bool,
    /// Optional live event sink for the interactive TUI transcript.
    pub ui_events: Option<mpsc::UnboundedSender<AgentUiEvent>>,
}

/// Run the agent with the given command.
pub async fn run_agent(opts: RunAgentOptions<'_>) -> Result<RunAgentResult> {
    let RunAgentOptions {
        command,
        system_prompt,
        user_prompt,
        config,
        cwd,
        print_mode,
        stream,
        verbose,
        mut session,
        is_followup,
        docs_snapshot_before,
        quiet,
        ui_events,
    } = opts;

    env::debug_log(format!("command={command} followup={is_followup}"));
    let start_time = Instant::now();
    let stream_text = stream || ui_events.is_some();
    let show_thinking = verbose;

    let (model, _models_arc, stream_fn) = resolve_model_and_auth(config, &ui_events).await?;
    let env = Arc::new(LocalExecutionEnv::new(cwd));

    let (mut agent_tools, base_tool_str) = if command == "chat" {
        (
            create_read_only_tools(env.clone()),
            "read, grep, find, ls (read-only mode)",
        )
    } else {
        (create_all_tools(env.clone()), "read, bash, edit, write, grep, find, ls")
    };

    if command == "chat" {
        agent_tools.push(create_ask_text_tool());
        agent_tools.push(create_ask_select_tool());
        agent_tools.push(create_ask_confirm_tool());
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
            model: Some(model),
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

    let generating = if quiet {
        None
    } else {
        Some(progress_spinner("Thinking..."))
    };
    let saw_any_delta = Arc::new(AtomicBool::new(false));

    if let Some(write_ctx) = turn_write_ctx {
        agent
            .subscribe(create_checkpoint_write_subscriber(write_ctx, ui_events.clone(), quiet))
            .await;
    }

    if let Some(tx) = ui_events.clone() {
        agent
            .subscribe(create_tui_event_subscriber(tx, stream_text, show_thinking))
            .await;
    } else if !quiet {
        agent
            .subscribe(create_event_subscriber(
                stream,
                verbose,
                generating.as_ref().expect("spinner").clone(),
                saw_any_delta.clone(),
            ))
            .await;
    }

    agent.prompt_text(user_prompt.to_string(), None).await?;
    agent.wait_for_idle().await;

    let elapsed = start_time.elapsed();
    let state = agent.state().await;

    if let Some(session) = session {
        session.save_messages(&state.messages, command).await?;
    }

    let docs_changed = if let Some(before) = docs_snapshot_before.as_ref() {
        let after = docs::create_snapshot(cwd)?;
        docs::has_changed(before, &after)
    } else {
        false
    };

    let completion_message = if print_mode && !stream {
        if let Some(elph_ai::Message::Assistant(assistant)) = state.messages.last().and_then(|m| m.as_llm())
            && !verbose
        {
            for block in &assistant.content {
                if let elph_ai::AssistantContentBlock::Text(t) = block {
                    print!("{}", t.text);
                    let _ = std::io::stdout().flush();
                }
            }
            println!();
            String::new()
        } else {
            String::new()
        }
    } else if quiet && ui_events.is_some() {
        String::new()
    } else if quiet {
        format!("Completed in {:.1}s", elapsed.as_secs_f64())
    } else {
        format!("\x1b[90mCompleted in {:.1}s\x1b[0m", elapsed.as_secs_f64())
    };

    if let Some(tx) = ui_events {
        let _ = tx.send(AgentUiEvent::RunCompleted {
            elapsed_secs: elapsed.as_secs_f64(),
        });
    }

    Ok(RunAgentResult {
        completion_message,
        docs_changed,
        skipped: false,
    })
}

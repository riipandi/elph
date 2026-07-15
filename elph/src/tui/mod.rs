//! iocraft-based TUI for Elph.
//!
//! Layout reference: `crates/elph-tui/examples/chat_layout.rs` (basic demo; not synced with production).
//! Zones (top → bottom): Header, Transcript, status row, Editor, Footer.

mod activity;
mod agent_bridge;
mod chrome;
mod editor;
mod footer;
mod header;
mod labels;
mod prompt_chrome;
mod session_prefs;
mod shell;
mod status_row;
mod theme;
mod transcript;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use iocraft::prelude::*;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::agent::agent_mode_from_setting;
use crate::agent::{AgentUiEvent, CodingAgentSession, CreateSessionOptions, create_coding_session_with_events};
use crate::platform::{Paths, Settings};
use crate::types::ThinkingLevel;

use chrome::read_git_branch;
use labels::{model_footer_label, project_footer_label};
use shell::MainShell;

/// Launch options for the interactive TUI.
#[derive(Debug, Clone, Default)]
pub struct TuiOptions {
    pub resume_id: Option<String>,
}

struct AgentBootstrap {
    session: Arc<CodingAgentSession>,
    ui_rx: Arc<Mutex<UnboundedReceiver<AgentUiEvent>>>,
}

async fn try_bootstrap_agent(paths: &Paths, settings: &Settings, resume_id: Option<&str>) -> Result<AgentBootstrap> {
    let cwd = std::env::current_dir().map_err(|e| anyhow::anyhow!("{e}"))?;
    let (session, ui_rx) = create_coding_session_with_events(CreateSessionOptions {
        paths,
        settings,
        cwd: &cwd,
        resume_id,
        provider_override: None,
        model_override: None,
    })
    .await?;
    Ok(AgentBootstrap {
        session: Arc::new(session),
        ui_rx: Arc::new(Mutex::new(ui_rx)),
    })
}

/// Launch the Elph TUI.
pub async fn run_tui(options: TuiOptions) -> Result<()> {
    let paths = Paths::resolve()?;
    Settings::ensure(&paths)?;
    let settings = Settings::load(&paths)?;

    let agent = try_bootstrap_agent(&paths, &settings, options.resume_id.as_deref()).await;
    let (agent_session, ui_events, demo_transcript, session_id, context_limit, supports_images) = match agent {
        Ok(agent) => {
            log::info!("agent session ready: {}", agent.session.session_id());
            let session_id = agent.session.session_id().to_string();
            let context_limit = agent.session.context_window() as u64;
            let supports_images = agent.session.supports_image_input();
            (
                Some(agent.session),
                Some(agent.ui_rx),
                false,
                session_id,
                context_limit,
                supports_images,
            )
        }
        Err(err) => {
            log::warn!("agent session unavailable, running demo shell: {err}");
            let session_id = options.resume_id.clone().unwrap_or_else(|| "demo".to_string());
            (None, None, true, session_id, 200_000, false)
        }
    };

    let model_label = model_footer_label(settings.session.provider_id.as_deref(), settings.session.model_id.as_deref());
    let git_branch = read_git_branch(paths.project_dir());
    let project_label = project_footer_label(&paths, git_branch.as_deref());

    element!(MainShell(
        resume_id: options.resume_id,
        session_id: session_id,
        initial_agent_mode: agent_mode_from_setting(&settings.session.agent_mode),
        initial_thinking_level: ThinkingLevel::from_setting(&settings.session.thinking_level),
        model_label: model_label,
        project_label: project_label,
        context_limit: context_limit,
        supports_images: supports_images,
        footer_token_display: settings.footer_token_display.clone(),
        sticky_scroll: settings.sticky_scroll,
        show_thinking: settings.show_thinking,
        agent_session: agent_session,
        ui_events: ui_events,
        demo_transcript: demo_transcript,
    ))
    .render_loop()
    .fullscreen()
    .enable_mouse_capture()
    .ignore_ctrl_c()
    .await?;
    Ok(())
}

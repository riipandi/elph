//! iocraft-based TUI for Elph.
//!
//! Zones (top → bottom): Header, Transcript, status row, Editor, Footer.

mod activity;
mod agent_bridge;
mod chrome;
mod focus;
mod labels;
mod prompt;
mod session_prefs;
mod shell;
mod slash_handler;
mod slash_palette;
mod theme;
mod tool_approval;
mod transcript;
mod user_question;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use iocraft::prelude::*;
use tokio::sync::mpsc::UnboundedReceiver;

use elph_agent::LocalExecutionEnv;

use crate::agent::agent_mode_from_setting;
use crate::agent::{
    AgentUiEvent, CodingAgentSession, CreateSessionOptions, create_coding_session_with_events, load_resources,
    slash_commands_for_palette,
};
use crate::extensions::ExtensionHost;
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

    let extension_host = ExtensionHost::new();
    if let Err(err) = ExtensionHost::ensure_dirs(&paths) {
        log::warn!("extension dirs unavailable: {err}");
    } else if let Err(err) = extension_host.reload(&paths, true) {
        log::warn!("extension reload failed: {err}");
    }

    let cwd = paths.project_dir().clone();
    let env = LocalExecutionEnv::new(&cwd);
    let bootstrap_resources = load_resources(&paths, &cwd, &env).await;
    let prompt_templates = bootstrap_resources.resources.prompt_templates.clone();
    let skills = bootstrap_resources.resources.skills.clone();
    let slash_commands =
        slash_commands_for_palette(Some(&extension_host.registry().read()), Some(&prompt_templates), Some(&skills));

    let agent = try_bootstrap_agent(&paths, &settings, options.resume_id.as_deref()).await;
    let skill_conflict_notice = crate::agent::format_skill_conflict_notice(&bootstrap_resources.skill_conflicts);
    let (agent_session, ui_events, session_id, context_limit, supports_images, bootstrap_notice) = match agent {
        Ok(agent) => {
            log::info!("agent session ready: {}", agent.session.session_id());
            let session_id = agent.session.session_id().to_string();
            let context_limit = agent.session.context_window() as u64;
            let supports_images = agent.session.supports_image_input();
            (
                Some(agent.session),
                Some(agent.ui_rx),
                session_id,
                context_limit,
                supports_images,
                skill_conflict_notice,
            )
        }
        Err(err) => {
            log::warn!("agent session unavailable: {err}");
            let session_id = options.resume_id.clone().unwrap_or_else(|| "unavailable".to_string());
            let notice = match skill_conflict_notice {
                Some(conflicts) => Some(format!("Agent unavailable: {err}\n\n{conflicts}")),
                None => Some(format!("Agent unavailable: {err}")),
            };
            (None, None, session_id, 200_000, false, notice)
        }
    };

    let model_label = model_footer_label(settings.session.provider_id.as_deref(), settings.session.model_id.as_deref());
    let git_branch = read_git_branch(paths.project_dir());
    let project_label = project_footer_label(&paths, git_branch.as_deref());

    element!(MainShell(
        session_id: session_id,
        bootstrap_notice: bootstrap_notice,
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
        extension_host: extension_host,
        slash_commands: slash_commands,
        prompt_templates: prompt_templates,
        skills: skills,
        cwd: cwd,
    ))
    .render_loop()
    .fullscreen()
    .enable_mouse_capture()
    .ignore_ctrl_c()
    .await?;
    Ok(())
}

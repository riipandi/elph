//! iocraft-based TUI for Elph.
//!
//! Zones (top → bottom): Header, Transcript, ephemeral banner + status row (+ inline dialogs), prompt chrome.

mod activity;
mod agent_bridge;
mod ask_user_tool_card;
mod chrome;
mod confetti;
mod file_picker;
mod focus;
mod inline_dialog;
mod labels;
mod model_option_list;
mod model_selector;
mod model_selector_bar;
mod model_selector_shell;
mod prompt;
mod session_prefs;
mod shell;
mod shell_submit;
mod slash_handler;
mod slash_palette;
mod startup;
mod status_dialog;
mod subagent_display;
mod system_prompt_dialog;
mod theme;
mod tool_approval;
mod tool_params;
mod transcript;
mod user_question;
mod user_question_bar;
mod user_question_option_list;

use std::sync::Arc;

use anyhow::Result;
use iocraft::prelude::*;

use elph_agent::LocalExecutionEnv;

use elph_ai::get_builtin_model;

use crate::agent::agent_mode_from_setting;
use crate::agent::{load_resources, resolve_provider_and_model, slash_commands_for_palette};
use crate::extensions::ExtensionHost;
use crate::platform::{Paths, Settings};
use crate::types::ThinkingLevel;

use chrome::read_git_footer_info;
use labels::model_footer_label;
use shell::MainShell;
use startup::{TuiBootstrapConfig, initial_startup_messages};

/// Launch options for the interactive TUI.
#[derive(Debug, Clone, Default)]
pub struct TuiOptions {
    pub resume_id: Option<String>,
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
    let execution_env = Arc::new(LocalExecutionEnv::new(&cwd));
    let env = execution_env.clone();
    let bootstrap_resources = load_resources(&paths, &cwd, &env).await;
    let prompt_templates = bootstrap_resources.resources.prompt_templates.clone();
    let skills = bootstrap_resources.resources.skills.clone();
    let skill_conflicts = bootstrap_resources.skill_conflicts.clone();
    let slash_commands =
        slash_commands_for_palette(Some(&extension_host.registry().read()), Some(&prompt_templates), Some(&skills));

    let session_id = options.resume_id.clone().unwrap_or_else(|| "starting…".to_string());
    let (boot_provider, boot_model_id) = resolve_provider_and_model(
        None,
        None,
        settings.session.provider_id.as_deref(),
        settings.session.model_id.as_deref(),
    )?;
    let boot_model = get_builtin_model(&boot_provider, &boot_model_id);
    let context_limit = boot_model
        .as_ref()
        .map(|model| model.context_window as u64)
        .unwrap_or(200_000);
    let supports_images = boot_model
        .as_ref()
        .map(|model| model.input.iter().any(|cap| cap == "image"))
        .unwrap_or(false);
    let bootstrap_config = TuiBootstrapConfig {
        paths: paths.clone(),
        settings: settings.clone(),
        resume_id: options.resume_id.clone(),
        preloaded_resources: bootstrap_resources,
    };
    let startup_messages = initial_startup_messages(&skill_conflicts);

    let model_label = model_footer_label(Some(&boot_provider), Some(&boot_model_id));
    let git_footer = read_git_footer_info(paths.project_dir());

    element!(MainShell(
        session_id: session_id,
        startup_messages: startup_messages,
        bootstrap: Some(bootstrap_config),
        initial_agent_mode: agent_mode_from_setting(&settings.session.agent_mode),
        initial_thinking_level: ThinkingLevel::from_setting(&settings.session.thinking_level),
        model_label: model_label,
        context_limit: context_limit,
        supports_images: supports_images,
        footer_token_display: settings.footer_token_display.clone(),
        colored_status_footer: settings.colored_status_footer,
        sticky_scroll: settings.sticky_scroll,
        show_thinking: settings.show_thinking,
        auto_expand_thinking: settings.auto_expand_thinking,
        agent_session: None,
        ui_events: None,
        extension_host: extension_host,
        slash_commands: slash_commands,
        prompt_templates: prompt_templates,
        skills: skills,
        cwd: cwd,
        execution_env: execution_env,
        paths: paths,
        file_picker_show_hidden: settings.file_picker.show_hidden_files,
        initial_git_footer: git_footer,
    ))
    .render_loop()
    .fullscreen()
    .enable_mouse_capture()
    .ignore_ctrl_c()
    .await?;
    Ok(())
}

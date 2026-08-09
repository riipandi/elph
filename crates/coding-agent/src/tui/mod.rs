//! iocraft-based TUI for Elph.
//!
//! Zones (top → bottom): Header, Transcript, ephemeral banner + status row (+ inline dialogs), prompt chrome.

mod activity;
mod agent_bridge;
pub(crate) mod api_error_display;
mod ask_user_tool_card;
pub(crate) mod chrome;
mod confetti;
mod file_picker;
mod focus;
mod inline_dialog;
pub(crate) mod item_selector;
mod item_selector_bar;
pub(crate) mod labels;
pub(crate) mod mcp_auth_dialog;
mod model_option_list;
mod model_selector;
mod model_selector_bar;
mod model_selector_shell;
mod notifier;
mod prompt;
mod prompt_history;
pub(crate) mod provider_connect_dialog;
pub(crate) mod provider_credential_store;
mod rename_dialog;
mod scoped_models;
mod scoped_models_bar;
mod scoped_models_shell;
mod scroll_text_dialog;
mod session_prefs;
mod shell;
mod shell_submit;
pub(crate) mod slash_handler;
mod slash_palette;
mod startup;
mod status_dialog;
mod subagent_display;
pub(crate) mod subagent_output_dialog;
mod system_prompt_dialog;
mod theme;
mod tool_approval;
pub(crate) mod tool_params;
pub(crate) mod transcript;
mod user_question;
mod user_question_bar;
mod user_question_option_list;

use std::sync::Arc;

use anyhow::Result;
use iocraft::prelude::*;

use elph_agent::LocalExecutionEnv;

use elph_ai::get_builtin_model;
use elph_tui::install_theme_config;

/// Closes in-process `shell_use` PTY sessions when the process exits.
///
/// `shell_use` sessions are process-global (like background shell tasks) and,
/// absent an explicit `close`, would otherwise outlive the agent turn. The
/// guard is held for the whole `cli::run` lifetime so a TUI/run/server/ACP
/// process tears down terminal sessions on exit.
pub struct ShellUseTeardownGuard;

impl Drop for ShellUseTeardownGuard {
    fn drop(&mut self) {
        elph_agent::close_shell_use_sessions();
    }
}

use crate::agent::{load_resources, resolve_provider_and_model, slash_commands_for_palette};
use crate::extensions::ExtensionHost;
use crate::platform::{Paths, Settings};
use crate::tui::transcript::LogDensity;
use crate::types::{AgentMode, ThinkingLevel};

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
    let slash_commands =
        slash_commands_for_palette(Some(&extension_host.registry().read()), Some(&prompt_templates), Some(&skills));

    let session_id = options.resume_id.clone().unwrap_or_else(|| "starting…".to_string());
    let (boot_provider, boot_model_id) =
        resolve_boot_model(&settings, &paths, &cwd, options.resume_id.as_deref()).await?;
    let boot_model = get_builtin_model(&boot_provider, &boot_model_id);
    let context_limit = boot_model
        .as_ref()
        .map(|model| model.context_window as u64)
        .unwrap_or(200_000);
    let supports_images = boot_model
        .as_ref()
        .map(|model| model.input.iter().any(|cap| cap == "image"))
        .unwrap_or(false);
    let startup_messages = initial_startup_messages(&bootstrap_resources);
    let bootstrap_config = TuiBootstrapConfig {
        paths: paths.clone(),
        settings: settings.clone(),
        resume_id: options.resume_id.clone(),
        model_override: options
            .resume_id
            .is_none()
            .then(|| format!("{boot_provider}/{boot_model_id}")),
        preloaded_resources: bootstrap_resources,
    };

    let model_label = model_footer_label(Some(&boot_provider), Some(&boot_model_id));
    let git_footer = read_git_footer_info(paths.project_dir());

    // Resolve ui.theme (auto|dark|light) + ui.themes overrides into the process theme.
    // Do not wrap MainShell in ContextProvider — root layout must stay fullscreen.
    let _ui_theme = install_theme_config(&settings.ui.theme_config());

    element!(MainShell(
        session_id: session_id,
        startup_messages: startup_messages,
        bootstrap: Some(bootstrap_config),
        // Live agent mode is per-session; new sessions always start in build.
        initial_agent_mode: AgentMode::Build,
        initial_thinking_level: {
            let raw = ThinkingLevel::from_setting(&settings.models.default_thinking_level);
            if let Some(model) = boot_model.as_ref() {
                raw.clamp_for_model(model)
            } else {
                raw
            }
        },
        model_label: model_label,
        context_limit: context_limit,
        supports_images: supports_images,
        footer_token_display: settings.ui.footer_token_display.clone(),
        colored_status_footer: settings.ui.colored_status_footer,
        sticky_scroll: settings.ui.sticky_scroll,
        show_thinking: settings.ui.show_thinking,
        auto_expand_thinking: settings.ui.auto_expand_thinking,
        density: LogDensity::from_setting(&settings.ui.density),
        agent_session: None,
        ui_events: None,
        extension_host: extension_host,
        slash_commands: slash_commands,
        prompt_templates: prompt_templates,
        skills: skills,
        cwd: cwd,
        execution_env: execution_env,
        paths: paths,
        file_picker_show_hidden: settings.ui.file_picker.show_hidden_files,
        allow_mode_change_while_busy: settings.ui.allow_mode_change_while_busy,
        initial_git_footer: git_footer,
    ))
    .render_loop()
    .fullscreen()
    .enable_mouse_capture()
    .ignore_ctrl_c()
    .await?;
    Ok(())
}

/// Resolve the `(provider, model)` a fresh session should boot on.
///
/// Priority:
/// 1. Explicit `ELPH_PROVIDER` / `ELPH_MODEL` env vars (already honored by
///    [`resolve_provider_and_model`]) — always win.
/// 2. The model last used in this project's sessions, when it still exists in the
///    catalog — a fresh session continues where the previous one left off.
/// 3. `models.defaultModel` from settings — used on first run (no saved sessions),
///    when the last session never recorded a model, or when the remembered model
///    was removed from the catalog.
///
/// `resume_id: Some(..)` short-circuits to settings default: the harness restores
/// the session's own model from its tree instead.
pub(crate) async fn resolve_boot_model(
    settings: &Settings,
    paths: &Paths,
    cwd: &std::path::Path,
    resume_id: Option<&str>,
) -> Result<(String, String)> {
    let (default_provider, default_model_id) = match settings.models.default_provider_and_model() {
        Some((p, m)) => (Some(p), Some(m)),
        None => (None, None),
    };
    let resolved_default =
        resolve_provider_and_model(None, None, default_provider.as_deref(), default_model_id.as_deref())?;

    // When resuming, the harness restores its own model from the session tree —
    // short-circuit to settings default so we don't override it.
    if resume_id.is_some() {
        return Ok(resolved_default);
    }

    // Explicit env vars always win. Resolve with overrides so the env values are
    // actually honored (not just used as a signal to return settings default).
    let env_provider = std::env::var("ELPH_PROVIDER").ok();
    let env_model = std::env::var("ELPH_MODEL").ok();
    if env_provider.is_some() || env_model.is_some() {
        return resolve_provider_and_model(
            env_provider.as_deref(),
            env_model.as_deref(),
            default_provider.as_deref(),
            default_model_id.as_deref(),
        );
    }

    let manager = crate::agent::SessionManager::new(paths, cwd)?;
    match manager.last_used_model().await {
        // Only adopt the remembered model when it still exists in the catalog.
        Ok(Some((provider, model_id))) if get_builtin_model(&provider, &model_id).is_some() => Ok((provider, model_id)),
        Ok(Some((provider, model_id))) => {
            log::warn!("last used model {provider}/{model_id} no longer in catalog; using settings default");
            Ok(resolved_default)
        }
        Ok(None) | Err(_) => Ok(resolved_default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{DEFAULT_MODEL_ID, DEFAULT_PROVIDER};
    use crate::platform::Paths;

    fn test_paths(label: &str) -> Paths {
        let root = std::env::temp_dir().join(format!(
            "elph-resolve-boot-model-test-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = root.join("config");
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(&project).expect("create project dir");
        std::fs::create_dir_all(&data).expect("create data dir");
        Paths::from_dirs(config, data, project)
    }

    fn settings_with_default_model(model: &str) -> Settings {
        let mut settings = Settings::defaults();
        settings.models.default_model = Some(model.to_string());
        settings
    }

    /// Clears env vars that could interfere with `resolve_boot_model` tests.
    fn clear_model_env_vars() {
        unsafe {
            std::env::remove_var("ELPH_PROVIDER");
            std::env::remove_var("ELPH_MODEL");
        }
    }

    #[tokio::test]
    async fn resolve_boot_model_resume_id_uses_settings_default() {
        clear_model_env_vars();
        let paths = test_paths("resume");
        let cwd = paths.project_dir().clone();
        let settings = settings_with_default_model("anthropic/claude-sonnet-4");

        let (provider, model) = resolve_boot_model(&settings, &paths, &cwd, Some("some-session-id"))
            .await
            .expect("resolve");

        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4");
    }

    #[tokio::test]
    async fn resolve_boot_model_env_provider_wins_over_last_used() {
        clear_model_env_vars();
        let paths = test_paths("env-provider");
        let cwd = paths.project_dir().clone();
        let settings = settings_with_default_model("anthropic/claude-sonnet-4");

        // Create a session with a different model so last_used_model returns Some.
        let manager = crate::agent::SessionManager::new(&paths, &cwd).expect("manager");
        let mut session = manager.create(None).await.expect("create session");
        session
            .append_model_change("openai", "gpt-5.6-luna")
            .await
            .expect("model change");

        // ELPH_MODEL should override the last-used model from session.
        unsafe {
            std::env::set_var("ELPH_MODEL", "xai/grok-4.5");
        }

        let (provider, model) = resolve_boot_model(&settings, &paths, &cwd, None)
            .await
            .expect("resolve");

        clear_model_env_vars();
        assert_eq!(provider, "xai");
        assert_eq!(model, "grok-4.5");
    }

    #[tokio::test]
    async fn resolve_boot_model_uses_last_used_when_in_catalog() {
        clear_model_env_vars();
        let paths = test_paths("last-used");
        let cwd = paths.project_dir().clone();
        let settings = settings_with_default_model("anthropic/claude-sonnet-4");

        let manager = crate::agent::SessionManager::new(&paths, &cwd).expect("manager");
        let mut session = manager.create(None).await.expect("create session");
        session
            .append_model_change("openai", "gpt-5.6-luna")
            .await
            .expect("model change");

        let (provider, model) = resolve_boot_model(&settings, &paths, &cwd, None)
            .await
            .expect("resolve");

        // Should prefer the last-used model over settings default.
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-5.6-luna");
    }

    #[tokio::test]
    async fn resolve_boot_model_falls_back_when_last_used_not_in_catalog() {
        clear_model_env_vars();
        let paths = test_paths("removed-model");
        let cwd = paths.project_dir().clone();
        let settings = settings_with_default_model("anthropic/claude-sonnet-4");

        let manager = crate::agent::SessionManager::new(&paths, &cwd).expect("manager");
        let mut session = manager.create(None).await.expect("create session");
        // Use a model id that does not exist in the builtin catalog.
        session
            .append_model_change("openai", "this-model-does-not-exist-xyz")
            .await
            .expect("model change");

        let (provider, model) = resolve_boot_model(&settings, &paths, &cwd, None)
            .await
            .expect("resolve");

        // Last-used model is gone from the catalog → fall back to settings default.
        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4");
    }

    #[tokio::test]
    async fn resolve_boot_model_no_sessions_uses_settings_default() {
        clear_model_env_vars();
        let paths = test_paths("no-sessions");
        let cwd = paths.project_dir().clone();
        let settings = settings_with_default_model("anthropic/claude-sonnet-4");

        // Do not create any sessions — last_used_model returns None.
        let (provider, model) = resolve_boot_model(&settings, &paths, &cwd, None)
            .await
            .expect("resolve");

        assert_eq!(provider, "anthropic");
        assert_eq!(model, "claude-sonnet-4");
    }

    #[tokio::test]
    async fn resolve_boot_model_no_sessions_no_default_uses_hardcoded_default() {
        clear_model_env_vars();
        let paths = test_paths("hardcoded-default");
        let cwd = paths.project_dir().clone();
        // Settings with no default model configured.
        let settings = Settings::defaults();

        let (provider, model) = resolve_boot_model(&settings, &paths, &cwd, None)
            .await
            .expect("resolve");

        // Falls back to the hardcoded DEFAULT_PROVIDER / DEFAULT_MODEL_ID.
        assert_eq!(provider, DEFAULT_PROVIDER);
        assert_eq!(model, DEFAULT_MODEL_ID);
    }
}

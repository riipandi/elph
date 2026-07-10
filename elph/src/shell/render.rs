use std::path::Path;
use std::sync::{Arc, Mutex};

use elph_tui::{
    ActivityState, FooterInfo, FooterTokenDisplay, PromptOpts, ShellChrome, ShellRegion, StatusBarInfo,
    default_run_config, disable_keyboard_enhancement, enable_keyboard_enhancement, read_git_diff_stats,
    render_agent_shell, render_chat_stream_with_agent, render_model_selector, render_plan_confirmation, render_prompt,
    render_session_selector, render_tool_approval, render_tree_navigator, sigint_channel,
};
use slt::Context;

use crate::platform::{exit_message, handle_prompt_interrupt};
use crate::shell::{ActiveOverlay, ElphApp};
use crate::tui::TurnDispatcher;

pub fn render_app(ui: &mut Context, app: &mut ElphApp) {
    app.poll_ui_events();
    app.handle_global_keys(ui);
    app.theme.apply_to(ui);

    let overlay = app.active_overlay;
    let overlay_items = app.overlay_items.clone();
    let overlay_visible = app.overlay_visible();
    let dialog_visible = app.plan_modal.visible || app.tool_modal.visible;

    let project_dir = app.project_dir.clone();
    let project_name = elph_tui::path_basename(&project_dir).to_string();
    let model_name = app.prompt.model_name.clone();
    let session_id = app.session_id.clone();
    let thinking = app.thinking.label();
    let branch = app.git_branch.clone();
    let branch_ref = branch.as_deref();
    let (git_additions, git_deletions) = read_git_diff_stats(Path::new(&project_dir));
    let model_ref = if model_name.is_empty() {
        None
    } else {
        Some(model_name.as_str())
    };

    let footer = FooterInfo {
        model_name: model_ref,
        provider: None,
        thinking_level: thinking,
        supports_images: false,
        cost_usd: 0.0,
        tokens_used: 0,
        context_pct: 0.0,
        context_limit: 200_000,
        token_display: FooterTokenDisplay::Both,
        project_dir: &project_name,
        session_id: &session_id,
        mode: app.prompt.mode,
        turn: app.turn,
        branch: branch_ref,
        git_additions,
        git_deletions,
    };

    let status_bar = StatusBarInfo {
        branch: branch_ref,
        directory: &project_dir,
        tokens_used: footer.tokens_used,
        context_limit: footer.context_limit,
        git_additions: footer.git_additions,
        git_deletions: footer.git_deletions,
        turn: app.turn.max(1),
        turn_total: None,
    };

    let input = app.prompt.value();
    app.chat.collapse = app.collapse.clone();

    if app.agent_running && !app.activity.visible {
        app.activity = ActivityState::responding();
    }

    let slash_commands = app.slash_commands.clone();
    let slash_palette = app.slash_palette.clone();
    let theme = app.theme;
    let agent_running = app.agent_running;

    let chrome = ShellChrome::composer(
        status_bar,
        footer,
        &input,
        &slash_commands,
        &slash_palette,
        agent_running,
        if agent_running && app.activity.visible {
            Some(app.activity.clone())
        } else {
            None
        },
        app.spinner.clone(),
    );

    render_agent_shell(ui, theme, chrome, |ui, region| match region {
        ShellRegion::Chat => {
            render_chat_stream_with_agent(ui, &mut app.chat, theme, agent_running);
        }
        ShellRegion::Input => {
            app.handle_prompt(ui);
            if !overlay_visible && !dialog_visible {
                render_prompt(
                    ui,
                    &mut app.prompt,
                    theme,
                    PromptOpts {
                        running: agent_running,
                        composer: true,
                        queued_count: app.prompt_queue.len(),
                        ..Default::default()
                    },
                );
            }
        }
    });

    if app.plan_modal.visible {
        render_plan_confirmation(ui, &app.plan_modal, app.theme);
    } else if app.tool_modal.visible {
        render_tool_approval(ui, &app.tool_modal, app.theme);
    } else if overlay_visible {
        match overlay {
            ActiveOverlay::Model => {
                let current = app.prompt.model_name.clone();
                render_model_selector(ui, &overlay_items, &current, &mut app.model_selector, true);
            }
            ActiveOverlay::Session => {
                render_session_selector(ui, &overlay_items, &mut app.session_selector, true);
            }
            ActiveOverlay::Tree => {
                render_tree_navigator(ui, &overlay_items, &mut app.tree_navigator, true);
            }
            ActiveOverlay::None => {}
        }
    }
}

pub async fn run_sigint_watcher(app: Arc<Mutex<ElphApp>>) {
    let mut sigint = sigint_channel();
    while sigint.recv().await {
        if let Ok(mut guard) = app.lock() {
            if guard.agent_running {
                guard.activity.request_cancel();
                TurnDispatcher::spawn_abort(Arc::clone(&guard.session));
            } else if handle_prompt_interrupt(&mut guard.prompt.textarea) {
                guard.should_exit = true;
            }
        }
    }
}

pub fn run_tui(resume_id: Option<String>) -> std::io::Result<()> {
    let _ = enable_keyboard_enhancement();
    struct KeyboardGuard;
    impl Drop for KeyboardGuard {
        fn drop(&mut self) {
            let _ = disable_keyboard_enhancement();
        }
    }
    let _guard = KeyboardGuard;

    let settings = crate::platform::Paths::resolve()
        .and_then(|paths| crate::platform::Settings::load(&paths))
        .map_err(std::io::Error::other)?;

    let app =
        elph_agent::block_on(ElphApp::bootstrap(settings, resume_id.as_deref())).map_err(std::io::Error::other)?;
    let app = Arc::new(Mutex::new(app));
    let watcher_app = Arc::clone(&app);

    std::thread::spawn(move || {
        elph_agent::block_on(run_sigint_watcher(watcher_app));
    });

    let config = default_run_config();
    slt::run_with(config, move |ui: &mut Context| {
        let mut guard = app.lock().expect("elph app lock");
        if guard.should_exit {
            let snapshot = {
                let session_id = guard.session_id.clone();
                let total_api_secs = guard.total_api_secs;
                let started_at = guard.started_at;
                let project_dir = guard.project_dir.clone();
                let session = Arc::clone(&guard.session);
                drop(guard);
                ElphApp::exit_snapshot_from(&session_id, total_api_secs, started_at, &project_dir, &session)
            };
            exit_message::record(snapshot);
            ui.quit();
            return;
        }
        render_app(ui, &mut guard);
    })
}

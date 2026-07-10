use elph_tui::{ActivityState, BannerInfo, FooterInfo, ShellChrome, ShellRegion, render_agent_shell};
use slt::Context;

use super::OwlyApp;
use crate::tui::banner::directory_display;
use crate::tui::chat_stream::render_owly_chat_stream;
use crate::tui::setup::render_setup_wizard;

pub fn render_owly_app(ui: &mut Context, app: &mut OwlyApp) {
    if !app.setup_complete {
        if let Some(credentials) = app.setup.handle_keys(ui) {
            app.complete_setup(credentials);
        }
        if let Some(err) = &app.setup_error {
            app.setup.set_error(err.clone());
        }
        render_setup_wizard(ui, &mut app.setup, app.theme);
        return;
    }

    app.handle_global_keys(ui);
    app.theme.apply_to(ui);

    let directory = directory_display(app.context.cwd());
    let version = env!("CARGO_PKG_VERSION");
    let model_name = app.model.clone();
    let provider_name = app.provider.clone();
    let session_id = app.session_id.clone();
    let tip = app.tip;
    let model = if model_name.is_empty() {
        None
    } else {
        Some(model_name.as_str())
    };
    let provider = if provider_name.is_empty() {
        None
    } else {
        Some(provider_name.as_str())
    };

    let banner = BannerInfo {
        app_name: "Owly",
        version,
        update_available: false,
        directory: &directory,
        model,
        provider,
        extensions: 0,
        commands: 0,
        skills: 0,
        tools: 0,
        mcp_connected: 0,
        mcp_total: 0,
        mcp_tools: 0,
        tip,
    };
    let footer = FooterInfo {
        model_name: model,
        provider,
        thinking_level: "high",
        supports_images: false,
        cost_usd: 0.0,
        tokens_used: 0,
        context_pct: 0.0,
        context_limit: 262_000,
        token_display: Default::default(),
        project_dir: &directory,
        session_id: &session_id,
        mode: app.prompt.mode,
        turn: app.turn,
        branch: None,
        git_additions: 0,
        git_deletions: 0,
    };

    if app.running && !app.activity.visible {
        app.activity = ActivityState::working();
    }

    let theme = app.theme;
    let running = app.running;
    let show_thinking = app.show_thinking;

    let chrome = ShellChrome::simple(
        banner,
        footer,
        running,
        if running && app.activity.visible {
            Some(app.activity.clone())
        } else {
            None
        },
        app.spinner.clone(),
    );

    render_agent_shell(ui, theme, chrome, |ui, region| match region {
        ShellRegion::Chat => {
            render_owly_chat_stream(
                ui,
                &mut app.chat,
                &app.entries,
                &app.live_tools,
                theme,
                show_thinking,
                running,
            );
        }
        ShellRegion::Input => {
            app.render_input(ui);
        }
    });
}

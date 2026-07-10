use elph_tui::{BannerInfo, FooterInfo, render_inline_shell, simple_banner_lines};
use slt::Context;

use super::OwlyApp;
use crate::tui::banner::directory_display;
use crate::tui::setup::render_setup_wizard;
use crate::tui::static_flush::{emit_banner, sync_transcript};

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

    sync_static_transcript(ui, app);

    app.handle_global_keys(ui);
    app.theme.apply_to(ui);

    let directory = directory_display(app.context.cwd());
    let model_name = app.model.clone();
    let provider_name = app.provider.clone();
    let session_id = app.session_id.clone();
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

    let theme = app.theme;
    let running = app.running;
    let activity = if running && app.activity.visible {
        Some(app.activity.clone())
    } else {
        None
    };
    let spinner = app.spinner.clone();

    let slash_input = app.prompt.value();
    let slash_commands = app.slash_commands.clone();
    let slash_palette = app.slash_palette.clone();

    render_inline_shell(
        ui,
        theme,
        footer,
        running,
        activity,
        spinner,
        Some(slash_input.as_str()),
        Some(slash_commands.as_slice()),
        Some(&slash_palette),
        |ui| {
            app.render_input(ui);
        },
    );
}

fn sync_static_transcript(ui: &mut Context, app: &mut OwlyApp) {
    if !app.banner_emitted {
        let directory = directory_display(app.context.cwd());
        let banner = BannerInfo {
            app_name: "Owly",
            version: env!("CARGO_PKG_VERSION"),
            update_available: false,
            directory: &directory,
            model: if app.model.is_empty() {
                None
            } else {
                Some(app.model.as_str())
            },
            provider: if app.provider.is_empty() {
                None
            } else {
                Some(app.provider.as_str())
            },
            extensions: 0,
            commands: 0,
            skills: 0,
            tools: 0,
            mcp_connected: 0,
            mcp_total: 0,
            mcp_tools: 0,
            tip: app.tip,
        };
        emit_banner(ui, &simple_banner_lines(banner));
        app.banner_emitted = true;
    }

    sync_transcript(
        ui,
        &mut app.transcript_flush,
        &app.entries,
        app.show_thinking,
        app.running,
    );
}

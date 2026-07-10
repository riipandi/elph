use elph_tui::{
    AgentMode, BannerInfo, BannerState, FooterInfo, FooterTokenDisplay, Theme, pick_tip, render_banner, render_footer,
};
use slt::TestBackend;

#[test]
fn full_banner_renders_welcome() {
    let mut backend = TestBackend::new(80, 24);
    let theme = Theme::dark();
    let info = BannerInfo {
        app_name: "Elph",
        version: "0.79.1",
        update_available: false,
        directory: "~/project",
        model: Some("Claude Sonnet 4.6"),
        provider: Some("anthropic"),
        extensions: 0,
        commands: 0,
        skills: 0,
        tools: 0,
        mcp_connected: 0,
        mcp_total: 0,
        mcp_tools: 0,
        tip: pick_tip("test-session"),
    };

    backend.render(|ui| {
        render_banner(ui, info, BannerState::default(), theme);
    });

    backend.assert_contains("Welcome to Elph");
    backend.assert_contains("Directory:");
    backend.assert_contains("Tip:");
}

#[test]
fn compact_banner_after_first_message() {
    let mut backend = TestBackend::new(60, 8);
    let theme = Theme::dark();
    let info = BannerInfo {
        app_name: "Elph",
        version: "0.1.0",
        update_available: false,
        directory: "~/work/dir",
        model: None,
        provider: None,
        extensions: 0,
        commands: 0,
        skills: 0,
        tools: 0,
        mcp_connected: 0,
        mcp_total: 0,
        mcp_tools: 0,
        tip: "tip",
    };

    backend.render(|ui| {
        render_banner(ui, info, BannerState { compact: true }, theme);
    });

    backend.assert_contains("Elph v0.1.0");
}

#[test]
fn footer_renders_model_and_mode() {
    let mut backend = TestBackend::new(100, 10);
    let theme = Theme::dark();
    let info = FooterInfo {
        model_name: Some("Claude Sonnet 4.6"),
        provider: Some("anthropic"),
        thinking_level: "high",
        supports_images: true,
        cost_usd: 0.0,
        tokens_used: 0,
        context_pct: 0.0,
        context_limit: 262_000,
        token_display: FooterTokenDisplay::Both,
        project_dir: "elph",
        session_id: "abcd12345",
        mode: AgentMode::Build,
        turn: 0,
        branch: Some("main"),
        git_additions: 0,
        git_deletions: 0,
    };

    backend.render(|ui| {
        render_footer(ui, info, theme);
    });

    backend.assert_contains("Claude Sonnet 4.6");
    backend.assert_contains("IMG");
    backend.assert_contains("0k");
    backend.assert_contains("0.0% (262k)");
    backend.assert_contains("turn: 0");
}

use elph_tui::{
    AgentMode, BannerInfo, BannerState, ChatStreamState, FooterInfo, FooterTokenDisplay, ShellChrome, ShellRegion,
    SlashPaletteState, Theme, default_activity_spinner, elph_builtin_commands, pick_tip, render_agent_shell,
    render_chat_stream, render_prompt,
};
use slt::TestBackend;

#[test]
fn full_shell_renders_banner_chat_and_footer() {
    let mut backend = TestBackend::new(80, 24);
    let theme = Theme::dark();
    let mut chat = ChatStreamState::with_messages(vec!["hello".into()]);
    let mut prompt = elph_tui::PromptState::new("model");
    let slash_palette = SlashPaletteState::default();
    let slash_commands = elph_builtin_commands();
    let spinner = default_activity_spinner();

    let banner = BannerInfo {
        app_name: "Elph",
        version: "0.1.0",
        update_available: false,
        directory: "~/project",
        model: Some("test-model"),
        provider: None,
        extensions: 0,
        commands: 0,
        skills: 0,
        tools: 0,
        mcp_connected: 0,
        mcp_total: 0,
        mcp_tools: 0,
        tip: pick_tip("shell-test"),
    };
    let footer = FooterInfo {
        model_name: Some("test-model"),
        provider: None,
        thinking_level: "high",
        supports_images: false,
        cost_usd: 0.0,
        tokens_used: 0,
        context_pct: 0.0,
        context_limit: 262_000,
        token_display: FooterTokenDisplay::Both,
        project_dir: "elph",
        session_id: "sess",
        mode: AgentMode::Build,
        turn: 0,
        branch: Some("main"),
        git_additions: 0,
        git_deletions: 0,
    };

    backend.render(|ui| {
        let chrome = ShellChrome::full(
            banner,
            BannerState { compact: true },
            footer,
            "",
            &slash_commands,
            &slash_palette,
            false,
            None,
            spinner.clone(),
        );
        render_agent_shell(ui, theme, chrome, |ui, region| match region {
            ShellRegion::Chat => render_chat_stream(ui, &mut chat, theme),
            ShellRegion::Input => render_prompt(ui, &mut prompt, theme, Default::default()),
        });
    });

    backend.assert_contains("Elph v0.1.0");
    backend.assert_contains("hello");
    backend.assert_contains("0k");
}

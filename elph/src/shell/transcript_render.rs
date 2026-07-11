//! Formats [`TranscriptEntry`] rows for the tuie transcript pane.

use elph_tui::{
    CollapseState, MarkdownTheme, Theme, ThemeMode, ToolExecutionState, TranscriptEntry, TranscriptRole,
    render_streaming_markdown_lines,
};

const USER_PREFIX: &str = "› ";
const DIAMOND: &str = "♦ ";

/// Options for flattening transcript entries into scrollable lines.
#[derive(Debug, Clone)]
pub struct TranscriptRenderOptions<'a> {
    pub show_thinking: bool,
    pub agent_running: bool,
    pub collapse: &'a CollapseState,
    pub live_tools: &'a [ToolExecutionState],
    pub width: u16,
    pub theme: Theme,
}

impl<'a> TranscriptRenderOptions<'a> {
    pub fn new(
        show_thinking: bool,
        agent_running: bool,
        collapse: &'a CollapseState,
        live_tools: &'a [ToolExecutionState],
        width: u16,
        theme: Theme,
    ) -> Self {
        Self {
            show_thinking,
            agent_running,
            collapse,
            live_tools,
            width,
            theme,
        }
    }
}

/// Flatten typed entries into scrollable transcript lines (Composer layout).
pub fn entries_to_lines(entries: &[TranscriptEntry], opts: &TranscriptRenderOptions<'_>) -> Vec<String> {
    let markdown = markdown_theme(opts.theme);
    let width = opts.width.max(20);
    let mut lines = Vec::new();
    let mut prev_role: Option<TranscriptRole> = None;

    for (index, entry) in entries.iter().enumerate() {
        if should_skip_while_running(entries, index, opts.agent_running) {
            continue;
        }
        let gap = section_gap(prev_role, entry.role);
        for _ in 0..gap {
            lines.push(String::new());
        }
        prev_role = Some(entry.role);
        lines.extend(entry_lines(entry, index, opts, width, markdown));
    }

    if opts.agent_running && !opts.live_tools.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        for tool in opts.live_tools {
            lines.push(format_live_tool_line(tool));
        }
    }

    lines
}

/// Legacy helper for tests and simple call sites.
pub fn entries_to_lines_simple(
    entries: &[TranscriptEntry],
    show_thinking: bool,
    agent_running: bool,
    collapse: &CollapseState,
) -> Vec<String> {
    entries_to_lines(
        entries,
        &TranscriptRenderOptions::new(show_thinking, agent_running, collapse, &[], 80, Theme::dark()),
    )
}

fn should_skip_while_running(entries: &[TranscriptEntry], index: usize, agent_running: bool) -> bool {
    if !agent_running {
        return false;
    }
    let entry = &entries[index];
    if entry.role != TranscriptRole::Assistant || entry.is_streaming {
        return false;
    }
    entries[..index]
        .iter()
        .any(|e| e.role == TranscriptRole::Assistant && e.is_streaming)
}

fn markdown_theme(theme: Theme) -> MarkdownTheme {
    match theme.mode {
        ThemeMode::Light => MarkdownTheme::light(),
        ThemeMode::Dark => MarkdownTheme::dark(),
    }
}

fn entry_lines(
    entry: &TranscriptEntry,
    index: usize,
    opts: &TranscriptRenderOptions<'_>,
    width: u16,
    markdown: MarkdownTheme,
) -> Vec<String> {
    match entry.role {
        TranscriptRole::User => format_user(&entry.content),
        TranscriptRole::Assistant => {
            assistant_lines(&entry.content, entry.is_streaming && opts.agent_running, width, markdown)
        }
        TranscriptRole::Thinking if opts.show_thinking => {
            let duration = entry.timestamp.as_deref().map(|_| "0.2").unwrap_or("…");
            let header = format!("{DIAMOND}Thought for {duration}s");
            if opts.collapse.is_expanded(index) && !entry.content.trim().is_empty() {
                std::iter::once(header)
                    .chain(entry.content.lines().map(str::to_string))
                    .collect()
            } else {
                vec![header]
            }
        }
        TranscriptRole::Thinking => Vec::new(),
        TranscriptRole::Tool => entry
            .tool
            .as_ref()
            .map(|tool| vec![format_tool_line(tool)])
            .unwrap_or_default(),
        TranscriptRole::System => vec![format!("{DIAMOND}{}", entry.content)],
    }
}

fn assistant_lines(content: &str, streaming: bool, width: u16, theme: MarkdownTheme) -> Vec<String> {
    if content.is_empty() && !streaming {
        return Vec::new();
    }
    render_streaming_markdown_lines(content, width, theme, streaming, streaming)
}

fn section_gap(prev: Option<TranscriptRole>, current: TranscriptRole) -> u32 {
    let Some(prev) = prev else {
        return 0;
    };
    match (prev, current) {
        (TranscriptRole::User, _) | (_, TranscriptRole::User) => 1,
        (TranscriptRole::Assistant, TranscriptRole::Assistant) => 0,
        _ => 1,
    }
}

fn format_user(message: &str) -> Vec<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut lines_iter = trimmed.lines();
    let Some(first) = lines_iter.next() else {
        return Vec::new();
    };
    let mut out = vec![format!("{USER_PREFIX}{first}")];
    for line in lines_iter {
        out.push(format!("  {line}"));
    }
    out
}

fn format_tool_line(tool: &ToolExecutionState) -> String {
    let label = tool_block_label(tool);
    let detail = tool_detail_line(tool);
    if detail.is_empty() {
        format!("{DIAMOND}{label}")
    } else {
        format!("{DIAMOND}{label}  {detail}")
    }
}

fn format_live_tool_line(tool: &ToolExecutionState) -> String {
    let mut line = format_tool_line(tool);
    if tool.status == elph_tui::ToolExecutionStatus::Running {
        line.push_str("  …");
    }
    line
}

fn tool_block_label(tool: &ToolExecutionState) -> &'static str {
    let name = tool.name.to_ascii_lowercase();
    if name.contains("edit") || name.contains("write") || name.contains("str_replace") || name.contains("patch") {
        "Edit"
    } else if name.contains("shell") || name.contains("bash") || name.contains("run") || name.contains("command") {
        "Run"
    } else {
        "Tool"
    }
}

fn tool_detail_line(tool: &ToolExecutionState) -> String {
    let args = tool.args_summary.trim();
    if args.is_empty() {
        return String::new();
    }
    if args.contains('/') || args.contains('.') {
        args.lines().next().unwrap_or(args).to_string()
    } else if args.chars().count() > 72 {
        format!("{}…", args.chars().take(69).collect::<String>())
    } else {
        args.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_tui::ToolExecutionStatus;

    fn opts<'a>(
        show_thinking: bool,
        agent_running: bool,
        collapse: &'a CollapseState,
        live_tools: &'a [ToolExecutionState],
    ) -> TranscriptRenderOptions<'a> {
        TranscriptRenderOptions::new(show_thinking, agent_running, collapse, live_tools, 80, Theme::dark())
    }

    #[test]
    fn user_entry_uses_composer_prefix() {
        let lines = entries_to_lines_simple(&[TranscriptEntry::user("hello")], true, false, &CollapseState::default());
        assert_eq!(lines, vec!["› hello".to_string()]);
    }

    #[test]
    fn streaming_assistant_renders_partial_text_while_running() {
        let entries = vec![
            TranscriptEntry::user("go"),
            TranscriptEntry::assistant_streaming("partial"),
            TranscriptEntry::assistant("done"),
        ];
        let lines = entries_to_lines(&entries, &opts(true, true, &CollapseState::default(), &[]));
        assert!(lines.iter().any(|l| l == "› go"));
        assert!(lines.iter().any(|l| l.contains("partial")));
        assert!(!lines.iter().any(|l| l == "done"));
    }

    #[test]
    fn live_tools_append_while_running() {
        let tool = ToolExecutionState::new("1", "bash")
            .with_args("cargo test")
            .with_status(ToolExecutionStatus::Running);
        let lines = entries_to_lines(
            &[TranscriptEntry::user("run tests")],
            &opts(false, true, &CollapseState::default(), std::slice::from_ref(&tool)),
        );
        assert!(lines.iter().any(|l| l.contains("Run") && l.contains("cargo test")));
        assert!(lines.iter().any(|l| l.ends_with('…')));
    }

    #[test]
    fn tool_line_includes_detail() {
        let tool = ToolExecutionState::new("1", "edit")
            .with_args("/src/main.rs")
            .with_status(ToolExecutionStatus::Success);
        let lines = entries_to_lines_simple(&[TranscriptEntry::tool(tool)], true, false, &CollapseState::default());
        assert_eq!(lines, vec!["♦ Edit  /src/main.rs".to_string()]);
    }

    #[test]
    fn multiline_user_indents_continuation() {
        let lines = entries_to_lines_simple(
            &[TranscriptEntry::user("first\nsecond")],
            true,
            false,
            &CollapseState::default(),
        );
        assert_eq!(lines, vec!["› first".to_string(), "  second".to_string()]);
    }

    #[test]
    fn thinking_collapsed_by_default() {
        let entry = TranscriptEntry::thinking("internal plan", false);
        let lines = entries_to_lines_simple(&[entry], true, false, &CollapseState::default());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with('♦'));
        assert!(!lines[0].contains("internal"));
    }

    #[test]
    fn system_entry_uses_diamond_prefix() {
        let lines =
            entries_to_lines_simple(&[TranscriptEntry::system("ready")], true, false, &CollapseState::default());
        assert_eq!(lines, vec!["♦ ready".to_string()]);
    }

    #[test]
    fn assistant_markdown_renders_heading() {
        let lines = entries_to_lines_simple(
            &[TranscriptEntry::assistant("# Title\n\nBody")],
            true,
            false,
            &CollapseState::default(),
        );
        let joined = lines.join("\n");
        assert!(joined.contains("Title"));
        assert!(joined.contains("Body"));
    }
}

//! Shared terminal colors for the Elph shell.
//!
//! Palette aligned with Pi `dark` theme and the
//! [256-color xterm reference](https://www.ditig.com/256-colors-cheat-sheet).

use elph_tui::InputPrefixKind;
use iocraft::prelude::Color;

use crate::types::AgentMode;

/// Pi `darkGray` / muted chrome — near xterm 239 Grey30 (`#4e4e4e`).
pub const BORDER_MUTED: Color = Color::Rgb { r: 80, g: 80, b: 80 };

/// xterm 236 Grey19 (`#303030`) — scrollbar track.
pub const SCROLLBAR_TRACK: Color = Color::Rgb { r: 48, g: 48, b: 48 };

/// xterm 240 Grey35 (`#585858`) — scrollbar thumb.
pub const SCROLLBAR_THUMB: Color = Color::Rgb { r: 88, g: 88, b: 88 };

/// Pi `text` — primary body foreground (`#d4d4d4`).
pub const TEXT_FG: Color = Color::Rgb { r: 212, g: 212, b: 212 };

/// Pi `userMessageBg` lineage — darker warm gray for user-submitted transcript bubbles.
pub const BUBBLE_BG: Color = Color::Rgb { r: 34, g: 33, b: 42 };

/// Alias for [`BUBBLE_BG`]; every user-originated prompt card uses this fill.
pub const USER_INPUT_BG: Color = BUBBLE_BG;

/// Left accent on user-input transcript cards — aligns with elph-tui `UiTheme.accent`.
pub const USER_INPUT_ACCENT: Color = Color::Rgb { r: 129, g: 161, b: 193 };

/// Process-row **task** title (Thinking / Edit / Subagent …) — white for clear scan.
pub const TOOL_TASK_LABEL_FG: Color = Color::White;

/// Process-row **parameter / target** (paths, patterns) — soft accent highlight only.
pub const TOOL_PARAM_HIGHLIGHT_FG: Color = USER_INPUT_ACCENT;

/// Pi `customMessageLabel` (`#9575cd`).
pub const SKILL_FG: Color = Color::Rgb { r: 149, g: 117, b: 205 };

/// Dim status lines in the transcript (model changes, slash echoes, agent status).
pub const META_FG: Color = Color::DarkGrey;

/// Ephemeral transcript toasts (`transient:*`) — soft amber/gold so mode changes and
/// short-lived notices read clearly against the dark transcript (not as urgent as quit).
pub const EPHEMERAL_NOTICE_FG: Color = Color::Rgb { r: 234, g: 196, b: 88 };

/// Quit-while-busy confirmation — warm orange (`#fab373`), matches status spinner accent.
pub const QUIT_BUSY_NOTICE_FG: Color = Color::Rgb { r: 250, g: 179, b: 115 };

/// Thinking blocks: no tinted card — foreground only (Pi `dim` / `thinkingText`).
pub const THINKING_BG: Color = Color::Reset;
pub const THINKING_FG: Color = Color::DarkGrey;

/// Pi `toolPendingBg` (`#282832`).
pub const TOOL_RUNNING_BG: Color = Color::Rgb { r: 40, g: 40, b: 50 };

/// Pi `toolOutput` / `gray` — xterm 244 Grey50 (`#808080`).
pub const TOOL_RUNNING_FG: Color = Color::Rgb { r: 128, g: 128, b: 128 };

/// Muted args line under the tool header.
pub const TOOL_ARGS_FG: Color = Color::Rgb { r: 160, g: 160, b: 160 };

/// Idle file picker row foreground — dimmer than [`TEXT_FG`].
pub const FILE_PICKER_ROW_IDLE_FG: Color = TOOL_RUNNING_FG;

/// Selected file picker row foreground — brighter than [`TEXT_FG`].
pub const FILE_PICKER_ROW_SELECTED_FG: Color = Color::White;

/// Selected file picker row background — aligns with elph-tui `dialog_selection_bg`.
pub const FILE_PICKER_ROW_SELECTED_BG: Color = Color::Rgb { r: 58, g: 52, b: 36 };

/// Fuzzy-match foreground for all file picker rows.
pub const FILE_PICKER_FUZZY_MATCH_FG: Color = USER_INPUT_ACCENT;

/// Dim body text for streamed/final tool output.
pub const TOOL_OUTPUT_FG: Color = Color::DarkGrey;

/// Soft success fill for tool cards (muted green wash).
pub const TOOL_SUCCESS_BG: Color = Color::Rgb { r: 36, g: 48, b: 40 };

/// Success status / tool done — soft clear green (readable, not olive-muddy).
pub const TOOL_SUCCESS_FG: Color = Color::Rgb { r: 146, g: 196, b: 136 };

/// Soft error fill for tool cards (muted red wash).
pub const TOOL_FAILED_BG: Color = Color::Rgb { r: 52, g: 36, b: 38 };

/// Failed status / tool error — soft red with clearer contrast on dark bg.
pub const TOOL_FAILED_FG: Color = Color::Rgb { r: 220, g: 118, b: 118 };

// ── Startup / MCP / subagent status-line palette (calmer than tool cards) ──
// Long scan lines (Agent ready, MCP servers) read better with lower chroma + normal weight.

/// In-progress status line (loading MCP, connecting) — soft amber, not warning-loud.
pub const STATUS_RUNNING_FG: Color = Color::Rgb { r: 196, g: 170, b: 120 };

/// Success status line (Agent ready, MCP connected) — muted sage green.
pub const STATUS_SUCCESS_FG: Color = Color::Rgb { r: 132, g: 168, b: 140 };

/// Failed status line — muted rose (clear but less aggressive than tool-fail red).
pub const STATUS_FAILED_FG: Color = Color::Rgb { r: 196, g: 128, b: 128 };

/// Queued / idle status line — dim grey.
pub const STATUS_QUEUED_FG: Color = TOOL_ARGS_FG;

pub const EDITOR_TEXT_FOCUSED: Color = Color::Grey;
pub const EDITOR_TEXT_DIMMED: Color = Color::DarkGrey;
pub const EDITOR_CURSOR: Color = Color::White;

/// Footer chrome dim (turn, brackets, separators, IMG).
pub const FOOTER_DIM_FG: Color = Color::DarkGrey;

/// Soft green for git additions — readable, not as vivid as agent Build green.
pub const FOOTER_GIT_ADD_FG: Color = Color::Rgb { r: 140, g: 185, b: 130 };

/// Soft red for git deletions — muted, aligned with soft footer accents.
pub const FOOTER_GIT_DEL_FG: Color = Color::Rgb { r: 200, g: 125, b: 125 };

pub const PROMPT_PREFIX_FG: Color = Color::White;
pub const PROMPT_BORDER_DEFAULT: Color = BORDER_MUTED;
pub const PROMPT_BORDER_SHELL: Color = Color::Rgb { r: 34, g: 197, b: 94 };

/// Border color for the prompt editor from input prefix kind and agent mode.
pub fn prompt_border_color(kind: InputPrefixKind, agent_mode: AgentMode, has_focus: bool) -> Color {
    let base = match kind {
        InputPrefixKind::ShellWithContext | InputPrefixKind::ShellNoContext => PROMPT_BORDER_SHELL,
        InputPrefixKind::Default | InputPrefixKind::Slash if agent_mode == AgentMode::Plan => {
            rgb_color(agent_mode.label_rgb())
        }
        InputPrefixKind::Default | InputPrefixKind::Slash => PROMPT_BORDER_DEFAULT,
    };
    if has_focus { base } else { dim_border_color(base) }
}

fn dim_border_color(color: Color) -> Color {
    match color {
        Color::Rgb { r, g, b } => Color::Rgb {
            r: ((r as u16 * 7) / 10) as u8,
            g: ((g as u16 * 7) / 10) as u8,
            b: ((b as u16 * 7) / 10) as u8,
        },
        other => other,
    }
}
/// Transcript panel top border when the scroll region has focus.
pub const TRANSCRIPT_BORDER_FOCUSED: Color = Color::Rgb { r: 120, g: 120, b: 120 };

pub fn rgb_color((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb { r, g, b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_mode_label_rgb_matches_footer_palette() {
        assert_eq!(AgentMode::Build.label_rgb(), (236, 234, 228));
        assert_eq!(AgentMode::Plan.label_rgb(), (204, 168, 52));
        assert_eq!(AgentMode::Ask.label_rgb(), (59, 130, 246));
        assert_eq!(AgentMode::Brave.label_rgb(), (249, 115, 22));
        let plan_border = rgb_color(AgentMode::Plan.label_rgb());
        assert_eq!(
            prompt_border_color(InputPrefixKind::Default, AgentMode::Plan, true),
            plan_border
        );
        assert_eq!(prompt_border_color(InputPrefixKind::Slash, AgentMode::Plan, true), plan_border);
        assert_eq!(
            prompt_border_color(InputPrefixKind::Default, AgentMode::Build, true),
            PROMPT_BORDER_DEFAULT
        );
    }

    #[test]
    fn thinking_level_border_rgb_matches_soft_strata() {
        use crate::types::ThinkingLevel;
        assert_eq!(ThinkingLevel::Off.border_rgb(), (156, 163, 175));
        assert_eq!(ThinkingLevel::Minimal.border_rgb(), (94, 200, 212));
        assert_eq!(ThinkingLevel::Low.border_rgb(), (123, 159, 212));
        assert_eq!(ThinkingLevel::Medium.border_rgb(), (212, 165, 116));
        assert_eq!(ThinkingLevel::High.border_rgb(), (220, 110, 118));
        assert_eq!(ThinkingLevel::Xhigh.border_rgb(), (180, 154, 217));
        assert_eq!(ThinkingLevel::Max.border_rgb(), (196, 138, 212));
    }

    #[test]
    fn user_input_bg_is_darker_than_legacy_bubble() {
        let lum = |c: Color| match c {
            Color::Rgb { r, g, b } => (r as u32 + g as u32 + b as u32) / 3,
            _ => 128,
        };
        let legacy = Color::Rgb { r: 52, g: 53, b: 65 };
        assert!(lum(USER_INPUT_BG) < lum(legacy));
    }
}

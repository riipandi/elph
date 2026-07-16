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

/// Pi `customMessageLabel` (`#9575cd`).
pub const SKILL_FG: Color = Color::Rgb { r: 149, g: 117, b: 205 };

/// Dim status lines in the transcript (model changes, slash echoes, agent status).
pub const META_FG: Color = Color::DarkGrey;

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

/// Pi `toolSuccessBg` (`#283228`).
pub const TOOL_SUCCESS_BG: Color = Color::Rgb { r: 40, g: 50, b: 40 };

/// Pi `success` / `green` (`#b5bd68`).
pub const TOOL_SUCCESS_FG: Color = Color::Rgb { r: 181, g: 189, b: 104 };

/// Pi `toolErrorBg` (`#3c2828`).
pub const TOOL_FAILED_BG: Color = Color::Rgb { r: 60, g: 40, b: 40 };

/// Pi `error` / `red` (`#cc6666`).
pub const TOOL_FAILED_FG: Color = Color::Rgb { r: 204, g: 102, b: 102 };

pub const EDITOR_TEXT_FOCUSED: Color = Color::Grey;
pub const EDITOR_TEXT_DIMMED: Color = Color::DarkGrey;
pub const EDITOR_CURSOR: Color = Color::White;

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
    fn plan_mode_prompt_border_matches_dark_green_label_rgb() {
        assert_eq!(AgentMode::Plan.label_rgb(), (22, 101, 52));
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
    fn user_input_bg_is_darker_than_legacy_bubble() {
        let lum = |c: Color| match c {
            Color::Rgb { r, g, b } => (r as u32 + g as u32 + b as u32) / 3,
            _ => 128,
        };
        let legacy = Color::Rgb { r: 52, g: 53, b: 65 };
        assert!(lum(USER_INPUT_BG) < lum(legacy));
    }
}

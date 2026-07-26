//! Shared terminal colors for the Elph shell.
//!
//! Aligned with the default Ghostty dark palette used by [`elph_tui::components::UiTheme`]:
//!
//! | Role | Hex |
//! |------|-----|
//! | background | `#181a1d` |
//! | foreground | `#d4d5d9` |
//! | selection | `#336ff1` |
//! | black / red / green / yellow | `#191a1c` / `#ff6b66` / `#8ed16a` / `#ffb347` |
//! | blue / magenta / cyan / white | `#6699ff` / `#d4aaff` / `#4dd0e1` / `#e0e2e8` |
//! | bright variants | `#7a7e85` … `#ffffff` |

use elph_tui::InputPrefixKind;
use iocraft::prelude::Color;

use crate::types::AgentMode;

// ── Ghostty base ──────────────────────────────────────────────────────────

/// Shell background `#181a1d` (for documentation / future fills; TUI often uses `Reset`).
#[allow(dead_code)]
pub const BACKGROUND: Color = Color::Rgb {
    r: 0x18,
    g: 0x1a,
    b: 0x1d,
};

/// Foreground `#d4d5d9`.
pub const TEXT_FG: Color = Color::Rgb {
    r: 0xd4,
    g: 0xd5,
    b: 0xd9,
};

/// Palette 8 `#7a7e85` — muted chrome / borders.
pub const BORDER_MUTED: Color = Color::Rgb {
    r: 0x7a,
    g: 0x7e,
    b: 0x85,
};

/// Palette 0 `#191a1c` — scrollbar track / near-black surfaces.
pub const SCROLLBAR_TRACK: Color = Color::Rgb {
    r: 0x19,
    g: 0x1a,
    b: 0x1c,
};

/// Elevated charcoal between bg and palette 8 — scrollbar thumb.
pub const SCROLLBAR_THUMB: Color = Color::Rgb {
    r: 0x3a,
    g: 0x3d,
    b: 0x42,
};

/// User-submitted transcript bubbles — slight lift over background.
pub const BUBBLE_BG: Color = Color::Rgb {
    r: 0x22,
    g: 0x24,
    b: 0x28,
};

/// Alias for [`BUBBLE_BG`]; every user-originated prompt card uses this fill.
pub const USER_INPUT_BG: Color = BUBBLE_BG;

/// Left accent on user-input cards — palette 4 `#6699ff` (matches `UiTheme.accent`).
pub const USER_INPUT_ACCENT: Color = Color::Rgb {
    r: 0x66,
    g: 0x99,
    b: 0xff,
};

/// Process-row **task** title — bright white (palette 15).
pub const TOOL_TASK_LABEL_FG: Color = Color::White;

/// Process-row **parameter / target** — soft blue accent.
pub const TOOL_PARAM_HIGHLIGHT_FG: Color = USER_INPUT_ACCENT;

/// Skills / custom labels — palette 5 `#d4aaff`.
pub const SKILL_FG: Color = Color::Rgb {
    r: 0xd4,
    g: 0xaa,
    b: 0xff,
};

/// Dim status lines in the transcript — palette 8.
pub const META_FG: Color = Color::Rgb {
    r: 0x7a,
    g: 0x7e,
    b: 0x85,
};

/// Ephemeral toasts — palette 11 `#ffd966` (bright yellow).
pub const EPHEMERAL_NOTICE_FG: Color = Color::Rgb {
    r: 0xff,
    g: 0xd9,
    b: 0x66,
};

/// Quit-while-busy confirmation — palette 3 `#ffb347`.
pub const QUIT_BUSY_NOTICE_FG: Color = Color::Rgb {
    r: 0xff,
    g: 0xb3,
    b: 0x47,
};

/// Thinking blocks: no tinted card — muted grey foreground.
pub const THINKING_BG: Color = Color::Reset;
pub const THINKING_FG: Color = Color::Rgb {
    r: 0x7a,
    g: 0x7e,
    b: 0x85,
};

/// Tool running wash — cool charcoal (bg + blue tint).
pub const TOOL_RUNNING_BG: Color = Color::Rgb {
    r: 0x1e,
    g: 0x22,
    b: 0x2a,
};

/// Tool running / pending label — palette 8.
pub const TOOL_RUNNING_FG: Color = Color::Rgb {
    r: 0x7a,
    g: 0x7e,
    b: 0x85,
};

/// Muted args line under the tool header.
pub const TOOL_ARGS_FG: Color = Color::Rgb {
    r: 0x9a,
    g: 0x9e,
    b: 0xa5,
};

/// Idle file picker row foreground.
pub const FILE_PICKER_ROW_IDLE_FG: Color = TOOL_RUNNING_FG;

/// Selected file picker row foreground — palette 15.
pub const FILE_PICKER_ROW_SELECTED_FG: Color = Color::White;

/// Selected file picker row background — warm amber wash (dialog selection family).
pub const FILE_PICKER_ROW_SELECTED_BG: Color = Color::Rgb {
    r: 0x3a,
    g: 0x32,
    b: 0x22,
};

/// Fuzzy-match foreground for file picker rows.
pub const FILE_PICKER_FUZZY_MATCH_FG: Color = USER_INPUT_ACCENT;

/// Dim body text for streamed/final tool output.
pub const TOOL_OUTPUT_FG: Color = Color::Rgb {
    r: 0x7a,
    g: 0x7e,
    b: 0x85,
};

/// Soft success fill for tool cards (green wash on bg).
pub const TOOL_SUCCESS_BG: Color = Color::Rgb {
    r: 0x1e,
    g: 0x28,
    b: 0x20,
};

/// Success status — palette 2 `#8ed16a`.
pub const TOOL_SUCCESS_FG: Color = Color::Rgb {
    r: 0x8e,
    g: 0xd1,
    b: 0x6a,
};

/// Soft error fill for tool cards (red wash on bg).
pub const TOOL_FAILED_BG: Color = Color::Rgb {
    r: 0x2c,
    g: 0x1e,
    b: 0x1e,
};

/// Failed status — palette 1 `#ff6b66`.
pub const TOOL_FAILED_FG: Color = Color::Rgb {
    r: 0xff,
    g: 0x6b,
    b: 0x66,
};

// ── Startup / MCP / subagent status-line palette ──────────────────────────

/// In-progress status — palette 3 softened.
pub const STATUS_RUNNING_FG: Color = Color::Rgb {
    r: 0xe0,
    g: 0xa8,
    b: 0x5c,
};

/// Success status — muted palette 2.
pub const STATUS_SUCCESS_FG: Color = Color::Rgb {
    r: 0x8e,
    g: 0xd1,
    b: 0x6a,
};

/// Failed status — muted palette 9 `#ff8a85`.
pub const STATUS_FAILED_FG: Color = Color::Rgb {
    r: 0xff,
    g: 0x8a,
    b: 0x85,
};

/// Queued / idle status line.
pub const STATUS_QUEUED_FG: Color = TOOL_ARGS_FG;

pub const EDITOR_TEXT_FOCUSED: Color = TEXT_FG;
pub const EDITOR_TEXT_DIMMED: Color = Color::Rgb {
    r: 0x7a,
    g: 0x7e,
    b: 0x85,
};
/// Ghostty `cursor-color` `#ffffff`.
pub const EDITOR_CURSOR: Color = Color::White;

/// Footer chrome dim (turn, brackets, separators, IMG).
pub const FOOTER_DIM_FG: Color = Color::Rgb {
    r: 0x7a,
    g: 0x7e,
    b: 0x85,
};

/// Git additions — palette 2.
pub const FOOTER_GIT_ADD_FG: Color = Color::Rgb {
    r: 0x8e,
    g: 0xd1,
    b: 0x6a,
};

/// Git deletions — palette 9 softened.
pub const FOOTER_GIT_DEL_FG: Color = Color::Rgb {
    r: 0xff,
    g: 0x8a,
    b: 0x85,
};

pub const PROMPT_PREFIX_FG: Color = Color::White;
pub const PROMPT_BORDER_DEFAULT: Color = BORDER_MUTED;
/// Shell `!` mode border — palette 2 green.
pub const PROMPT_BORDER_SHELL: Color = Color::Rgb {
    r: 0x8e,
    g: 0xd1,
    b: 0x6a,
};

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
pub const TRANSCRIPT_BORDER_FOCUSED: Color = Color::Rgb {
    r: 0x7a,
    g: 0x7e,
    b: 0x85,
};

pub fn rgb_color((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb { r, g, b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_mode_label_rgb_matches_footer_palette() {
        assert_eq!(AgentMode::Build.label_rgb(), (0xe0, 0xe2, 0xe8));
        assert_eq!(AgentMode::Plan.label_rgb(), (0xff, 0xb3, 0x47));
        assert_eq!(AgentMode::Ask.label_rgb(), (0x66, 0x99, 0xff));
        assert_eq!(AgentMode::Brave.label_rgb(), (0xff, 0x8a, 0x4d));
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
    fn thinking_level_border_rgb_matches_ghostty_strata() {
        use crate::types::ThinkingLevel;
        assert_eq!(ThinkingLevel::Off.border_rgb(), (0x7a, 0x7e, 0x85));
        assert_eq!(ThinkingLevel::Minimal.border_rgb(), (0x4d, 0xd0, 0xe1));
        assert_eq!(ThinkingLevel::Low.border_rgb(), (0x9b, 0xc4, 0xff));
        assert_eq!(ThinkingLevel::Medium.border_rgb(), (0xff, 0xb3, 0x47));
        assert_eq!(ThinkingLevel::High.border_rgb(), (0xff, 0x6b, 0x66));
        assert_eq!(ThinkingLevel::Xhigh.border_rgb(), (0xd4, 0xaa, 0xff));
        assert_eq!(ThinkingLevel::Max.border_rgb(), (0xe8, 0xb4, 0xff));
    }

    #[test]
    fn user_input_bg_is_darker_than_legacy_bubble() {
        let lum = |c: Color| match c {
            Color::Rgb { r, g, b } => (r as u32 + g as u32 + b as u32) / 3,
            _ => 128,
        };
        let legacy = Color::Rgb { r: 52, g: 53, b: 65 };
        assert!(lum(USER_INPUT_BG) < lum(legacy));
        assert!(lum(USER_INPUT_BG) > lum(BACKGROUND));
    }

    #[test]
    fn ghostty_primary_tokens() {
        assert_eq!(
            TEXT_FG,
            Color::Rgb {
                r: 0xd4,
                g: 0xd5,
                b: 0xd9
            }
        );
        assert_eq!(
            USER_INPUT_ACCENT,
            Color::Rgb {
                r: 0x66,
                g: 0x99,
                b: 0xff
            }
        );
        assert_eq!(
            TOOL_SUCCESS_FG,
            Color::Rgb {
                r: 0x8e,
                g: 0xd1,
                b: 0x6a
            }
        );
        assert_eq!(
            TOOL_FAILED_FG,
            Color::Rgb {
                r: 0xff,
                g: 0x6b,
                b: 0x66
            }
        );
        assert_eq!(EDITOR_CURSOR, Color::White);
    }
}

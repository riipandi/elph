//! Shared terminal colors for the Elph shell.
//!
//! Palette aligned with Pi `dark` theme and the
//! [256-color xterm reference](https://www.ditig.com/256-colors-cheat-sheet).

use iocraft::prelude::Color;

/// Pi `darkGray` / muted chrome — near xterm 239 Grey30 (`#4e4e4e`).
pub const BORDER_MUTED: Color = Color::Rgb { r: 80, g: 80, b: 80 };

/// xterm 236 Grey19 (`#303030`) — scrollbar track.
pub const SCROLLBAR_TRACK: Color = Color::Rgb { r: 48, g: 48, b: 48 };

/// xterm 240 Grey35 (`#585858`) — scrollbar thumb.
pub const SCROLLBAR_THUMB: Color = Color::Rgb { r: 88, g: 88, b: 88 };

/// Pi `text` — primary body foreground (`#d4d4d4`).
pub const TEXT_FG: Color = Color::Rgb { r: 212, g: 212, b: 212 };

/// Pi `userMessageBg` (`#343541`).
pub const BUBBLE_BG: Color = Color::Rgb { r: 52, g: 53, b: 65 };

/// Pi `customMessageBg` (`#2d2838`).
pub const SKILL_BG: Color = Color::Rgb { r: 45, g: 40, b: 56 };

/// Pi `customMessageLabel` (`#9575cd`).
pub const SKILL_FG: Color = Color::Rgb { r: 149, g: 117, b: 205 };

/// Pi export `infoBg` (`#3c3728`).
pub const META_BG: Color = Color::Rgb { r: 60, g: 55, b: 40 };

/// Pi `mdHeading` (`#f0c674`).
pub const META_FG: Color = Color::Rgb { r: 240, g: 198, b: 116 };

/// Thinking blocks: no tinted card — foreground only (Pi `dim` / `thinkingText`).
pub const THINKING_BG: Color = Color::Reset;
pub const THINKING_FG: Color = Color::DarkGrey;

/// Pi `toolPendingBg` (`#282832`).
pub const TOOL_RUNNING_BG: Color = Color::Rgb { r: 40, g: 40, b: 50 };

/// Pi `toolOutput` / `gray` — xterm 244 Grey50 (`#808080`).
pub const TOOL_RUNNING_FG: Color = Color::Rgb { r: 128, g: 128, b: 128 };

/// Muted args line under the tool header.
pub const TOOL_ARGS_FG: Color = Color::Rgb { r: 160, g: 160, b: 160 };

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

/// Pi `borderMuted` (`#505050`).
pub const EDITOR_BORDER: Color = Color::Rgb { r: 80, g: 80, b: 80 };
/// Muted editor chrome when transcript has focus.
pub const EDITOR_BORDER_DIMMED: Color = Color::Rgb { r: 56, g: 56, b: 56 };
pub const EDITOR_TEXT_FOCUSED: Color = Color::Grey;
pub const EDITOR_TEXT_DIMMED: Color = Color::DarkGrey;
pub const EDITOR_CURSOR: Color = Color::White;
/// Transcript panel top border when the scroll region has focus.
pub const TRANSCRIPT_BORDER_FOCUSED: Color = Color::Rgb { r: 120, g: 120, b: 120 };

pub fn rgb_color((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb { r, g, b }
}

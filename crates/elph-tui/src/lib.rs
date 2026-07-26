//! Terminal UI components for Elph agent.
//!
//! OpenTUI-inspired component APIs implemented with [iocraft](https://crates.io/crates/iocraft).
//!
//! @ref: https://opentui.com/docs/getting-started

pub mod cli_progress;
pub mod clipboard;
pub mod color;
pub mod components;
pub mod input_prefix;
pub mod loader;
pub mod paste;
pub mod slash_palette;
pub mod text_editing;
pub mod text_input_layout;
pub mod theme_config;
pub mod transcript_layout;
pub mod types;
pub mod utils;

pub use cli_progress::{CliProgress, CliSpinner};
pub use cli_progress::{progress_enabled, progress_spinner};
pub use clipboard::{ClipboardCopyStatus, ClipboardNotice, copy_to_clipboard, copy_with_status, read_from_clipboard};
pub use color::{from_hex, from_rgb_fn, parse_color, parse_color_value, rgb};
pub use components::*;
pub use input_prefix::{
    DEFAULT_PROMPT_PREFIX_GLYPH, InputPrefixKind, LIST_SELECTION_MARKER, LIST_SELECTION_ROW_PREFIX_IDLE,
    LIST_SELECTION_ROW_PREFIX_SELECTED, PREFIX_COLUMN_WIDTH, PromptPrefixConfig, absorb_inline_triggers,
    backspace_trigger_kind, compose_palette_draft, detect_input_prefix, effective_prefix_kind,
    list_selection_row_prefix, prefix_symbol, resolve_submit_draft, strip_body_triggers, strip_submit_trigger,
    try_consume_trigger,
};
pub use loader::{KittScanner, KittScannerConfig, LoaderCell, SPINNER_FRAME_MS, SpinnerLoader};
pub use slash_palette::{
    PaletteSnapshot, SlashCommand, SlashPaletteKeyAction, build_snapshot, complete_command, filter_commands,
    open_palette_draft, palette_anchor_bottom, palette_list_height, palette_query, palette_visible,
    resolve_snapshot_key_action, sync_selection,
};
pub use theme_config::{
    ThemeAppearance, ThemeConfig, ThemeMode, ThemePalettes, ThemeTokenOverrides, active_ui_theme,
    clear_active_ui_theme, detect_terminal_appearance, install_theme_config, set_active_ui_theme, try_active_ui_theme,
};
pub use types::{
    DialogAgentMode, DialogTodoItem, DialogTodoProgress, DialogTodoProgressItem, DialogTodoStatus, SelectOption,
    TabItem,
};

/// Convenience re-exports for application authors.
pub mod prelude {
    pub use crate::color::{from_hex, parse_color, rgb};
    pub use crate::components::*;
    pub use crate::theme_config::{ThemeConfig, ThemeMode, ThemePalettes, install_theme_config};
    pub use crate::types::{
        DialogAgentMode, DialogTodoItem, DialogTodoProgress, DialogTodoProgressItem, DialogTodoStatus, SelectOption,
        TabItem,
    };
    pub use iocraft::prelude::*;
}

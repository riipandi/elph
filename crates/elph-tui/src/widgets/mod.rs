//! tuie widget building blocks for the agent shell.

pub mod chrome_tuie;
pub mod command_palette;
pub mod focus_pane;
pub mod prompt;
pub mod sidebar;
pub mod streaming_text;
pub mod transcript;

pub(crate) use chrome_tuie::{ActivityHandles, FooterHandles};
pub use chrome_tuie::{ActivityPane, FooterPane, build_activity_widget, build_footer_widget};
pub use command_palette::{
    CommandPaletteState, build_palette_widget, close_palette_popup, filtered_command_count, open_palette_popup,
    palette_selection_text, palette_visible, refresh_palette_list,
};
pub(crate) use prompt::PromptHandles;
pub use prompt::PromptPane;
pub use sidebar::SidebarPlaceholder;
pub use streaming_text::StreamingText;
pub(crate) use transcript::TranscriptHandles;
pub use transcript::TranscriptPane;

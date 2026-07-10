//! Bridge between diff-TUI components and SLT agent UI.

mod key_encode;
mod overlay_render;
mod overlay_state;

pub use key_encode::key_event_to_terminal_data;
pub use overlay_render::render_diff_overlay;
pub use overlay_state::{OverlaySlot, OverlayStack};

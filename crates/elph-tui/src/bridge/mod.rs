//! Bridge between diff-TUI components and iocraft agent UI.

mod key_encode;
mod overlay_state;
mod portal;

pub use key_encode::key_event_to_terminal_data;
pub use overlay_state::{OverlaySlot, OverlayStack};
pub use portal::{DiffOverlayPortal, DiffOverlayPortalProps, OverlayStackHandle};

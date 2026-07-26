//! Prompt input history palette (Arrow Up while the editor is focused).
//!
//! | Module      | Responsibility                                      |
//! |-------------|-----------------------------------------------------|
//! | `model`     | History store helpers + render snapshot             |
//! | `keyboard`  | Map key presses to palette actions                  |
//! | `component` | Floating palette UI above the editor                |

mod component;
mod keyboard;
mod model;

pub use component::PromptHistoryPalette;
pub use keyboard::PromptHistoryKeyAction;
pub use keyboard::is_open_key;
pub use keyboard::resolve_key_action;
pub use model::PromptHistorySnapshot;
pub use model::build_snapshot;
pub use model::can_open_history;
pub use model::push_history_entry;
pub use model::seed_history_from_transcript;

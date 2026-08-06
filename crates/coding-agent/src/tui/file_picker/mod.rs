//! `@` file mention picker — fuzzy workspace search via `fff-search`.

mod apply;
mod component;
mod fuzzy_highlight;
mod highlight;
mod keyboard;
mod model;
mod state;

pub use apply::FilePickerApplyContext;
pub use apply::apply_file_picker_key;

pub use component::FilePickerPalette;
pub use highlight::mention_highlight_ansi;
pub use keyboard::FilePickerKeyAction;
pub use keyboard::resolve_key_action;
pub use model::FilePickerSnapshot;
pub use model::active_mention_at_cursor;
pub use model::build_snapshot;
pub use model::cursor_after_mention_complete;
pub use model::cursor_after_mention_dismiss;
pub use model::file_picker_open;
pub use model::mention_picker_visible;
pub use state::sync_selection;

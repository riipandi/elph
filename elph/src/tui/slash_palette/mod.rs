//! Slash command autocomplete palette.
//!
//! | Module        | Responsibility                                     |
//! |---------------|----------------------------------------------------|
//! | `model`       | Derive snapshot from draft + command registry      |
//! | `layout`      | Anchor palette above editor (no textarea overlap)  |
//! | `card`        | Bordered panel — chrome, header, body, [`SlashPaletteCard`] |
//! | `keyboard`    | Map key presses to palette actions                 |
//! | `state`       | Sync shell selection when the filter query changes |
//! | `component`   | Floating [`SlashCommandPalette`] shell             |

mod card;
mod component;
mod fuzzy;
mod keyboard;
mod layout;
mod model;
mod row_layout;
mod state;

pub use component::SlashCommandPalette;
pub use keyboard::{SlashPaletteKeyAction, resolve_snapshot_key_action};
pub use layout::palette_anchor_bottom;
pub use model::{SlashPaletteSnapshot, build_snapshot};
pub use state::sync_selection;

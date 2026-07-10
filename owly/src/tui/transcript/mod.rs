//! Applies [`AgentUiEvent`] updates to typed [`OwlyEntry`] transcript lists.

mod applier;
mod helpers;

pub use applier::TranscriptApplier;
pub use helpers::{append_shell_lines, lines_to_entries};

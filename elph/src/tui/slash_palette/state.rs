//! Selection state kept by the shell while the palette is active.

use super::model::{SlashPaletteSnapshot, clamp_index};

/// Reset and clamp palette selection when the filter query or match list changes.
pub fn sync_selection(tracked_query: &mut String, selected_index: &mut usize, snapshot: &SlashPaletteSnapshot) {
    if *tracked_query != snapshot.query {
        *tracked_query = snapshot.query.clone();
        *selected_index = 0;
    }
    if snapshot.has_matches() {
        *selected_index = clamp_index(*selected_index, snapshot.options.len());
    } else {
        *selected_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SlashCommand;

    use super::super::model::build_snapshot;

    #[test]
    fn resets_selection_when_query_changes() {
        let commands = vec![SlashCommand::new("goal", "Goals"), SlashCommand::new("help", "Help")];
        let mut query = String::new();
        let mut index = 1usize;

        sync_selection(&mut query, &mut index, &build_snapshot("/hel", &commands, 40));
        assert_eq!(index, 0);

        sync_selection(&mut query, &mut index, &build_snapshot("/go", &commands, 40));
        assert_eq!(index, 0);
    }
}

//! Sync shell selection when the `@` filter query changes.

use super::model::FilePickerSnapshot;

pub fn sync_selection(query: &mut String, index: &mut usize, snapshot: &FilePickerSnapshot) {
    if !snapshot.visible {
        query.clear();
        *index = 0;
        return;
    }
    if snapshot.query != *query {
        *query = snapshot.query.clone();
        *index = 0;
    }
    let len = snapshot.options.len();
    if len == 0 {
        *index = 0;
    } else if *index >= len {
        *index = len - 1;
    }
}

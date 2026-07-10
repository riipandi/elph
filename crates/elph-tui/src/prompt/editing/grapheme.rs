use slt::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

/// Grapheme-cluster count (matches SLT cursor indexing).
pub(super) fn grapheme_count(s: &str) -> usize {
    s.graphemes(true).count()
}

pub(super) fn byte_index_for_grapheme(s: &str, cluster_index: usize) -> usize {
    if cluster_index == 0 {
        return 0;
    }
    s.grapheme_indices(true)
        .nth(cluster_index)
        .map_or(s.len(), |(idx, _)| idx)
}

pub(super) fn cluster_is_alphanumeric(cluster: &str) -> bool {
    cluster.chars().next().is_some_and(|c| c.is_alphanumeric())
}

pub(super) fn prev_word_col(line: &str, col: usize) -> usize {
    let clusters: Vec<&str> = line.graphemes(true).collect();
    let mut pos = col.min(clusters.len());
    while pos > 0 && !cluster_is_alphanumeric(clusters[pos - 1]) {
        pos -= 1;
    }
    while pos > 0 && cluster_is_alphanumeric(clusters[pos - 1]) {
        pos -= 1;
    }
    pos
}

pub(super) fn next_word_col(line: &str, col: usize) -> usize {
    let clusters: Vec<&str> = line.graphemes(true).collect();
    let mut pos = col.min(clusters.len());
    while pos < clusters.len() && !cluster_is_alphanumeric(clusters[pos]) {
        pos += 1;
    }
    while pos < clusters.len() && cluster_is_alphanumeric(clusters[pos]) {
        pos += 1;
    }
    pos
}

pub(super) fn is_newline_key(key: &KeyEvent) -> bool {
    if key.kind != KeyEventKind::Press {
        return false;
    }
    match key.code {
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => true,
        KeyCode::Char('\n') => true,
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) && (c == 'j' || c == 'J') => true,
        _ => false,
    }
}

pub(super) fn has_word_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL)
        || modifiers.contains(KeyModifiers::ALT)
        || modifiers.contains(KeyModifiers::SUPER)
        || modifiers.contains(KeyModifiers::META)
}

pub(super) fn is_super_only(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::SUPER)
        && !modifiers.contains(KeyModifiers::ALT)
        && !modifiers.contains(KeyModifiers::CONTROL)
}

pub(super) fn is_word_nav_modifier(modifiers: KeyModifiers) -> bool {
    if is_super_only(modifiers) {
        return false;
    }
    modifiers.contains(KeyModifiers::ALT)
        || modifiers.contains(KeyModifiers::CONTROL)
        || modifiers.contains(KeyModifiers::META)
}

pub(super) fn is_delete_word_modifier(modifiers: KeyModifiers) -> bool {
    has_word_modifier(modifiers)
}

pub(super) fn is_plain_navigation(modifiers: KeyModifiers) -> bool {
    modifiers == KeyModifiers::NONE
}

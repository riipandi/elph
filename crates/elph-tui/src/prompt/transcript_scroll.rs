//! Sticky-tail scrolling for transcript [`ScrollState`] (chat log pattern).

use slt::{Context, Event, KeyCode, KeyEventKind, KeyModifiers, ScrollState};

/// Rows from the bottom still considered "pinned" (markdown reflow tolerance).
pub const BOTTOM_TOLERANCE: usize = 2;

/// Snapshot taken immediately before rendering a scrollable transcript.
#[derive(Debug, Clone, Copy)]
pub struct ScrollSnapshot {
    pub offset: usize,
    pub max_offset: usize,
    pub was_at_bottom: bool,
}

impl ScrollSnapshot {
    pub fn capture(scroll: &ScrollState) -> Self {
        let max_offset = max_scroll_offset(scroll);
        Self {
            offset: scroll.offset,
            max_offset,
            was_at_bottom: is_pinned_to_bottom(scroll),
        }
    }
}

pub fn max_scroll_offset(scroll: &ScrollState) -> usize {
    scroll.content_height().saturating_sub(scroll.viewport_height()) as usize
}

pub fn is_pinned_to_bottom(scroll: &ScrollState) -> bool {
    let max = max_scroll_offset(scroll);
    scroll.offset >= max.saturating_sub(BOTTOM_TOLERANCE)
}

pub fn scroll_to_bottom(scroll: &mut ScrollState) {
    scroll.set_offset(max_scroll_offset(scroll));
}

/// Snap to the previous tail before [`Context::scroll_col`] measures new content.
///
/// Uses bounds from the prior frame so streaming updates start from the bottom
/// instead of lagging one frame behind.
pub fn prepare_transcript_follow(
    scroll: &mut ScrollState,
    auto_scroll: bool,
    follow_tail: bool,
    before: ScrollSnapshot,
) {
    if auto_scroll || (follow_tail && before.was_at_bottom) {
        scroll_to_bottom(scroll);
    }
}

/// Handle keyboard scrolling for the transcript. Plain arrows stay on the prompt.
pub fn handle_transcript_scroll_keys(
    ui: &mut Context,
    scroll: &mut ScrollState,
    scroll_h: &mut ScrollState,
    auto_scroll: &mut bool,
    line_step: usize,
    page_step: usize,
) {
    let shift = KeyModifiers::SHIFT;

    if key_code_with_modifiers(ui, KeyCode::Up, shift) {
        scroll.scroll_up(line_step);
        *auto_scroll = false;
    }
    if key_code_with_modifiers(ui, KeyCode::Down, shift) {
        scroll.scroll_down(line_step);
        if is_pinned_to_bottom(scroll) {
            *auto_scroll = true;
        }
    }
    if ui.key_code(KeyCode::PageUp) {
        scroll.scroll_up(page_step);
        *auto_scroll = false;
    }
    if ui.key_code(KeyCode::PageDown) {
        scroll.scroll_down(page_step);
        if is_pinned_to_bottom(scroll) {
            *auto_scroll = true;
        }
    }
    if key_code_with_modifiers(ui, KeyCode::End, shift) {
        scroll_to_bottom(scroll);
        *auto_scroll = true;
    }

    // Horizontal transcript scroll (wide tables, code blocks, CJK lines).
    let h_step = line_step.max(4);
    if key_code_with_modifiers(ui, KeyCode::Left, shift) {
        scroll_h.scroll_left(h_step);
    }
    if key_code_with_modifiers(ui, KeyCode::Right, shift) {
        scroll_h.scroll_right(h_step);
    }
}

/// Apply sticky-tail behaviour after [`Context::scroll_col`] has measured content.
///
/// When `follow_tail` is true (streaming / agent running), keeps the viewport on
/// the latest lines only while the user was already pinned to the tail.
/// A deliberate scroll upward (keyboard or mouse wheel) unpins; content growth
/// alone does not.
fn user_scrolled_up(current_offset: usize, before: ScrollSnapshot, max: usize) -> bool {
    let clamped_to_max = max > 0 && before.max_offset > max && current_offset == max && before.offset > max;
    !clamped_to_max && current_offset + 1 < before.offset
}

fn should_follow_tail(auto_scroll: bool, follow_tail: bool, was_at_bottom: bool) -> bool {
    auto_scroll || (follow_tail && was_at_bottom)
}

/// Unpin sticky tail after deliberate user scroll (keyboard or mouse wheel).
pub fn unpin_auto_scroll_if_scrolled_up(scroll: &ScrollState, auto_scroll: &mut bool, before: ScrollSnapshot) {
    if before.max_offset == 0 {
        return;
    }
    let max = max_scroll_offset(scroll);
    if user_scrolled_up(scroll.offset, before, max) {
        *auto_scroll = false;
        return;
    }
    if before.was_at_bottom && !is_pinned_to_bottom(scroll) {
        *auto_scroll = false;
    }
}

pub fn apply_transcript_auto_scroll(
    scroll: &mut ScrollState,
    auto_scroll: &mut bool,
    before: ScrollSnapshot,
    follow_tail: bool,
) {
    let max = max_scroll_offset(scroll);

    if user_scrolled_up(scroll.offset, before, max) {
        *auto_scroll = false;
        return;
    }

    if max == 0 {
        return;
    }

    if !*auto_scroll && follow_tail && !before.was_at_bottom {
        return;
    }

    if should_follow_tail(*auto_scroll, follow_tail, before.was_at_bottom) {
        scroll.set_offset(max);
    }

    if follow_tail && is_pinned_to_bottom(scroll) && *auto_scroll {
        *auto_scroll = true;
    }
}

pub fn key_code_with_modifiers(ui: &Context, code: KeyCode, modifiers: KeyModifiers) -> bool {
    ui.events().any(|event| {
        matches!(
            event,
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    && key.code == code
                    && key.modifiers.contains(modifiers)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scroll_is_pinned() {
        let scroll = ScrollState::new();
        assert!(is_pinned_to_bottom(&scroll));
        assert_eq!(max_scroll_offset(&scroll), 0);
    }

    #[test]
    fn detects_deliberate_scroll_up() {
        let before = ScrollSnapshot {
            offset: 40,
            max_offset: 40,
            was_at_bottom: true,
        };
        assert!(user_scrolled_up(30, before, 40));
        assert!(!user_scrolled_up(25, before, 25));
    }

    #[test]
    fn follow_tail_respects_prior_unpin() {
        assert!(!should_follow_tail(false, true, false));
        assert!(should_follow_tail(true, true, true));
    }

    #[test]
    fn prepare_follow_only_when_pinned() {
        let mut scroll = ScrollState::new();
        prepare_transcript_follow(
            &mut scroll,
            false,
            true,
            ScrollSnapshot {
                offset: 5,
                max_offset: 10,
                was_at_bottom: false,
            },
        );
        assert_eq!(scroll.offset, 0);

        prepare_transcript_follow(
            &mut scroll,
            true,
            false,
            ScrollSnapshot {
                offset: 0,
                max_offset: 0,
                was_at_bottom: true,
            },
        );
        assert_eq!(scroll.offset, 0);
    }
}

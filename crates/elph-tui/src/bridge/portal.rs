//! iocraft portal that embeds diff overlay components.

use std::rc::Rc;
use std::sync::Mutex;

use iocraft::prelude::*;

use super::overlay_state::OverlayStack;

/// Shared overlay stack for mounting diff components inside iocraft trees.
#[derive(Clone)]
pub struct OverlayStackHandle(Rc<Mutex<OverlayStack>>);

impl Default for OverlayStackHandle {
    fn default() -> Self {
        Self(Rc::new(Mutex::new(OverlayStack::new())))
    }
}

impl OverlayStackHandle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, OverlayStack> {
        self.0.lock().expect("overlay stack mutex poisoned")
    }
}

#[derive(Default, Props)]
pub struct DiffOverlayPortalProps {
    /// When false, the portal does not capture keys or render overlay content.
    pub visible: bool,
    /// When true, render only the focused overlay component (no full-screen composite).
    pub compact: bool,
    /// Optional dimmed backdrop behind the overlay.
    pub show_backdrop: bool,
}

/// Embeds a diff [`OverlayStack`] inside an iocraft layout.
///
/// Parent components populate the stack via [`OverlayStackHandle`] before or during render.
#[component]
pub fn DiffOverlayPortal(mut hooks: Hooks, props: &mut DiffOverlayPortalProps) -> impl Into<AnyElement<'static>> {
    let stack = hooks.use_context::<OverlayStackHandle>();
    let (term_width, term_height) = hooks.use_terminal_size();
    let visible = props.visible;
    let compact = props.compact;
    let show_backdrop = props.show_backdrop;
    // Input routing: call [`OverlayStackHandle::lock`].handle_input from the parent
    // component's `use_terminal_events` hook (overlay components are not `Send`).
    let lines = if visible {
        let mut guard = stack.lock();
        if compact {
            guard.render_focused(term_width, term_height)
        } else {
            guard.render(term_width, term_height)
        }
    } else {
        Vec::new()
    };

    let content = lines.join("\n");

    element! {
        View(
            width: 100pct,
            height: 100pct,
            position: Position::Absolute,
            top: 0,
            left: 0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            background_color: if show_backdrop && visible { Some(Color::DarkGrey) } else { None },
            flex_grow: 1.0,
        ) {
            #(if visible && !content.is_empty() {
                Some(element! {
                    Text(content: content)
                })
            } else {
                None
            })
        }
    }
}

//! Centered modal overlay wrapping [`super::DialogShell`].

use crate::components::theme::{UiTheme, resolve_ui_theme};
use iocraft::prelude::*;

use super::chrome::DialogChrome;
use super::frame::DialogShell;
use super::header::DialogHeader;

/// Vertical offset to center a dialog of the given outer height.
pub fn dialog_overlay_top(screen_height: u16, dialog_height: u16) -> u16 {
    screen_height.saturating_sub(dialog_height) / 2
}

/// Horizontal offset to center a dialog of the given width.
pub fn dialog_overlay_left(screen_width: u16, dialog_width: u16) -> u16 {
    screen_width.saturating_sub(dialog_width) / 2
}

/// Props for [`DialogShellOverlay`].
#[derive(Props)]
pub struct DialogShellOverlayProps<'a> {
    pub screen_width: u16,
    pub screen_height: u16,
    pub chrome: DialogChrome,
    pub header: DialogHeader,
    pub theme: Option<UiTheme>,
    /// Click handler for the header `[esc]` label.
    pub on_esc: HandlerMut<'static, ()>,
    pub children: Vec<AnyElement<'a>>,
}

impl<'a> Default for DialogShellOverlayProps<'a> {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            chrome: DialogChrome::default(),
            header: DialogHeader::title("Dialog"),
            theme: None,
            on_esc: HandlerMut::default(),
            children: Vec::new(),
        }
    }
}

/// Full-screen overlay layer with a centered [`DialogShell`].
///
/// Renders transparently outside the dialog card so underlying UI remains visible.
#[component]
pub fn DialogShellOverlay<'a>(props: &mut DialogShellOverlayProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let chrome = props.chrome.clone().with_theme(theme);
    let header = props.header.clone();
    let children = std::mem::take(&mut props.children);
    let on_esc = props.on_esc.take();
    element! {
        View(
            width: props.screen_width,
            height: props.screen_height,
            position: Position::Absolute,
            top: 0,
            left: 0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        ) {
            DialogShell(
                chrome: chrome,
                header: header,
                theme: Some(theme),
                on_esc: on_esc,
            ) {
                #(children)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::chrome::dialog_shell_estimated_height;
    use super::*;

    #[test]
    fn centers_dialog_on_screen() {
        assert_eq!(dialog_overlay_left(80, 40), 20);
        assert_eq!(dialog_overlay_top(24, 10), 7);
    }

    #[test]
    fn overlay_top_uses_outer_height() {
        let chrome = DialogChrome::default();
        let outer = dialog_shell_estimated_height(&chrome);
        assert_eq!(dialog_overlay_top(24, outer), 24u16.saturating_sub(outer) / 2);
    }
}

//! Bordered dialog shell with inline header and content slot.

use crate::components::theme::{UiTheme, resolve_ui_theme};
use iocraft::prelude::*;

use super::chrome::{DialogChrome, dialog_divider_line, dialog_shell_body_height, dialog_shell_chrome_rows};
use super::header::{DialogHeader, DialogHeaderRow};

/// Props for [`DialogShell`].
#[derive(Props)]
pub struct DialogShellProps<'a> {
    pub chrome: DialogChrome,
    pub header: DialogHeader,
    pub theme: Option<UiTheme>,
    /// Click handler for the header `[esc]` label (keyboard Esc remains app-owned).
    pub on_esc: HandlerMut<'static, ()>,
    /// When set, a `[copy]` label is shown before `[esc]` and invokes this handler.
    /// Defaults to a no-op handler (no button rendered).
    pub on_copy: Option<HandlerMut<'static, ()>>,
    pub children: Vec<AnyElement<'a>>,
}

impl<'a> Default for DialogShellProps<'a> {
    fn default() -> Self {
        Self {
            chrome: DialogChrome::default(),
            header: DialogHeader::title("Dialog"),
            theme: None,
            on_esc: HandlerMut::default(),
            on_copy: None,
            children: Vec::new(),
        }
    }
}

/// Reusable dialog frame: header row, divider, and composable body.
#[component]
pub fn DialogShell<'a>(props: &mut DialogShellProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let chrome = props.chrome.clone().with_theme(theme);
    let header = props.header.clone();
    let children = std::mem::take(&mut props.children);
    let on_esc = props.on_esc.take();
    let on_copy = props.on_copy.take();
    let divider = dialog_divider_line(chrome.content_width());
    let body_height = dialog_shell_body_height(&chrome);
    let chrome_rows = dialog_shell_chrome_rows(&chrome);

    element! {
        View(
            width: chrome.width,
            min_height: body_height.saturating_add(chrome_rows),
            border_style: BorderStyle::Round,
            border_color: chrome.border_color,
            background_color: chrome.background,
            padding_top: chrome.padding_top,
            padding_bottom: chrome.padding_bottom,
            padding_left: chrome.padding_horizontal,
            padding_right: chrome.padding_horizontal,
            flex_direction: FlexDirection::Column,
            gap: 0,
            position: Position::Relative,
            flex_shrink: 0f32,
        ) {
            View(
                width: chrome.content_width(),
                padding_bottom: chrome.header_gap,
                flex_shrink: 0f32,
            ) {
                DialogHeaderRow(
                    chrome: chrome.clone(),
                    header: header,
                    theme: Some(theme),
                    on_esc: on_esc,
                    on_copy: on_copy,
                )
            }
            #(if chrome.show_divider {
                Some(element! {
                    View(
                        width: chrome.content_width(),
                        padding_bottom: chrome.body_gap,
                        flex_shrink: 0f32,
                    ) {
                        Text(
                            content: divider,
                            color: chrome.muted_color,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                })
            } else {
                None
            })
            View(
                width: chrome.content_width(),
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
                align_items: AlignItems::FlexStart,
            ) {
                #(children)
            }
        }
    }
}

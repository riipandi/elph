//! Dialog header variants (title, search, tabs).

use crate::components::Input;
use crate::components::tab_select::{tab_select_clamped_index, tab_select_key_to_index};
use crate::components::theme::{UiTheme, resolve_ui_theme, tab_styles};
use iocraft::prelude::*;

use super::chrome::{DialogChrome, dialog_header_title_fit};

/// Header row above the dialog divider.
#[derive(Clone)]
pub enum DialogHeader {
    Title {
        text: String,
    },
    Search {
        placeholder: String,
        value: Option<State<String>>,
        has_focus: bool,
    },
    Tabs {
        labels: Vec<String>,
        selected: Option<State<usize>>,
        has_focus: bool,
    },
}

impl DialogHeader {
    pub fn title(text: impl Into<String>) -> Self {
        Self::Title { text: text.into() }
    }

    pub fn search(placeholder: impl Into<String>, value: Option<State<String>>, has_focus: bool) -> Self {
        Self::Search {
            placeholder: placeholder.into(),
            value,
            has_focus,
        }
    }

    pub fn tabs(labels: Vec<String>, selected: Option<State<usize>>, has_focus: bool) -> Self {
        Self::Tabs {
            labels,
            selected,
            has_focus,
        }
    }
}

#[derive(Clone, Default, Props)]
pub struct DialogHeaderTitleProps {
    pub chrome: DialogChrome,
    pub text: String,
    pub theme: Option<UiTheme>,
}

#[component]
pub fn DialogHeaderTitle(props: &DialogHeaderTitleProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = resolve_ui_theme(&hooks, props.theme);
    let fitted = dialog_header_title_fit(&props.text, props.chrome.content_width(), &props.chrome.esc_hint);
    element! {
        Text(
            content: fitted,
            color: props.chrome.title_color,
            weight: Weight::Bold,
            wrap: TextWrap::NoWrap,
        )
    }
}

#[derive(Clone, Default, Props)]
pub struct DialogHeaderSearchProps {
    pub chrome: DialogChrome,
    pub placeholder: String,
    pub value: Option<State<String>>,
    pub has_focus: bool,
    pub theme: Option<UiTheme>,
}

#[component]
pub fn DialogHeaderSearch(props: &DialogHeaderSearchProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal = hooks.use_state(String::new);
    let theme = resolve_ui_theme(&hooks, props.theme);
    let search_width = props
        .chrome
        .inner_body_width()
        .saturating_sub(theme.input_inset().saturating_mul(2))
        .max(8);
    let prefix = if props.placeholder.is_empty() {
        "› ".to_string()
    } else {
        format!("› {} ", props.placeholder)
    };

    element! {
        View(flex_direction: FlexDirection::Row, gap: 0) {
            Text(content: prefix, color: props.chrome.muted_color, wrap: TextWrap::NoWrap)
            Input(
                width: search_width,
                initial_value: String::new(),
                has_focus: props.has_focus,
                value: props.value.unwrap_or(internal),
                text_color: Some(props.chrome.title_color),
                focused_border_color: Some(props.chrome.border_color),
                theme: Some(theme),
            )
        }
    }
}

#[derive(Clone, Default, Props)]
pub struct DialogHeaderTabsProps {
    pub chrome: DialogChrome,
    pub labels: Vec<String>,
    pub selected: Option<State<usize>>,
    pub has_focus: bool,
    pub theme: Option<UiTheme>,
}

#[component]
pub fn DialogHeaderTabs(props: &DialogHeaderTabsProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let internal = hooks.use_state(|| 0usize);
    let mut index_state = props.selected.unwrap_or(internal);
    let len = props.labels.len();
    let has_focus = props.has_focus;

    hooks.use_terminal_events(move |event| {
        if !has_focus || len == 0 {
            return;
        }
        let TerminalEvent::Key(KeyEvent { code, kind, .. }) = event else {
            return;
        };
        if kind == KeyEventKind::Release {
            return;
        }
        index_state.set(tab_select_key_to_index(index_state.get(), len, code));
    });

    let index = tab_select_clamped_index(index_state.get(), len);
    let theme = resolve_ui_theme(&hooks, props.theme);
    let tabs: Vec<_> = props
        .labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let active = i == index;
            let (border_style, color, weight) = tab_styles(theme, active);
            element! {
                View(
                    border_style: border_style,
                    border_color: color,
                    padding_left: theme.padding_sm,
                    padding_right: theme.padding_sm,
                ) {
                    Text(
                        content: label.clone(),
                        color: color,
                        weight: weight,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
        })
        .collect();

    element! {
        View(flex_direction: FlexDirection::Row, gap: theme.gap_md, flex_wrap: FlexWrap::Wrap) {
            #(tabs)
        }
    }
}

#[derive(Props)]
pub struct DialogHeaderRowProps {
    pub chrome: DialogChrome,
    pub header: DialogHeader,
    pub theme: Option<UiTheme>,
    /// When set, the `[esc]` label is clickable and invokes this handler (mouse down).
    pub on_esc: HandlerMut<'static, ()>,
}

impl Default for DialogHeaderRowProps {
    fn default() -> Self {
        Self {
            chrome: DialogChrome::default(),
            header: DialogHeader::title("Dialog"),
            theme: None,
            on_esc: HandlerMut::default(),
        }
    }
}

/// Full header row: left variant + clickable esc hint.
#[component]
pub fn DialogHeaderRow(props: &mut DialogHeaderRowProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = resolve_ui_theme(&hooks, props.theme);
    let chrome = props.chrome.clone();
    let left: AnyElement<'static> = match &props.header {
        DialogHeader::Title { text } => element! {
            DialogHeaderTitle(chrome: chrome.clone(), text: text.clone(), theme: Some(theme))
        }
        .into(),
        DialogHeader::Search {
            placeholder,
            value,
            has_focus,
        } => element! {
            DialogHeaderSearch(
                chrome: chrome.clone(),
                placeholder: placeholder.clone(),
                value: *value,
                has_focus: *has_focus,
                theme: Some(theme),
            )
        }
        .into(),
        DialogHeader::Tabs {
            labels,
            selected,
            has_focus,
        } => element! {
            DialogHeaderTabs(
                chrome: chrome.clone(),
                labels: labels.clone(),
                selected: *selected,
                has_focus: *has_focus,
                theme: Some(theme),
            )
        }
        .into(),
    };

    let esc_hint = chrome.esc_hint.clone();
    let esc_color = chrome.muted_color;
    // Mouse-only: keyboard Esc is still handled by the app shell.
    let on_esc = props.on_esc.take();

    element! {
        View(
            width: chrome.content_width(),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
        ) {
            View(flex_grow: 1f32, flex_shrink: 1f32, min_width: 0) {
                #(left)
            }
            Button(handler: on_esc, has_focus: false) {
                Text(
                    content: esc_hint,
                    color: esc_color,
                    wrap: TextWrap::NoWrap,
                )
            }
        }
    }
}

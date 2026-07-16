//! Multiline prompt editor with dynamic prefix and agent-mode overlap label.

use elph_tui::Textarea;
use elph_tui::components::UiTheme;
use elph_tui::{InputPrefixKind, PREFIX_COLUMN_WIDTH, PromptPrefixConfig, effective_prefix_kind, prefix_symbol};
use iocraft::prelude::*;

use crate::types::AgentMode;

use crate::tui::theme::prompt_border_color;
use crate::tui::theme::rgb_color;
use crate::tui::theme::{EDITOR_CURSOR, EDITOR_TEXT_DIMMED, EDITOR_TEXT_FOCUSED, PROMPT_PREFIX_FG};

fn editor_max_height(screen_height: u16) -> u16 {
    (screen_height / 4).clamp(4, 12)
}

#[derive(Default, Props)]
pub struct EditorProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub has_focus: bool,
    pub prefix_config: PromptPrefixConfig,
    pub input_prefix_kind: Option<Ref<InputPrefixKind>>,
    pub draft: Option<State<String>>,
    pub live_draft: Option<Ref<String>>,
    pub suppress_enter_newline: Option<Ref<bool>>,
    pub slash_palette_active: Option<Ref<bool>>,
    pub force_palette_sync: Option<Ref<bool>>,
    pub force_clear: Option<Ref<bool>>,
    pub on_submit: HandlerMut<'static, String>,
    pub on_escape: HandlerMut<'static, ()>,
    /// Shown centered when the editor is blocked by an inline dialog.
    pub blocked_hint: Option<String>,
}

#[component]
pub fn Editor(props: &mut EditorProps) -> impl Into<AnyElement<'static>> {
    let theme = UiTheme::default();
    let label_color = rgb_color(props.agent_mode.label_rgb());
    let has_focus = props.has_focus;
    let draft_text = props
        .live_draft
        .as_ref()
        .map(|live| live.read().clone())
        .or_else(|| props.draft.as_ref().map(|draft| draft.read().clone()))
        .unwrap_or_default();
    let stored_kind = props
        .input_prefix_kind
        .as_ref()
        .map(|kind| kind.get())
        .unwrap_or(InputPrefixKind::Default);
    let prefix_kind = effective_prefix_kind(stored_kind, &draft_text, &props.prefix_config);
    let border_color = prompt_border_color(prefix_kind, props.agent_mode, has_focus);
    let inset = theme.shell_zone_padding();
    let inner_width = theme.shell_editor_inner_width(props.screen_width);
    let prefix_cols = if props.prefix_config.enabled {
        PREFIX_COLUMN_WIDTH
    } else {
        0
    };
    let textarea_width = inner_width.saturating_sub(prefix_cols).max(1);
    let text_color = if has_focus {
        EDITOR_TEXT_FOCUSED
    } else {
        EDITOR_TEXT_DIMMED
    };
    let prefix_label = if props.prefix_config.enabled {
        format!("{} ", prefix_symbol(prefix_kind, &props.prefix_config))
    } else {
        String::new()
    };

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            border_style: BorderStyle::Round,
            border_color: border_color,
            position: Position::Relative,
            align_items: AlignItems::FlexStart,
            margin_bottom: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: inset,
            padding_right: inset,
        ) {
            View(
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexStart,
                width: inner_width,
            ) {
                #(props.prefix_config.enabled.then(|| -> AnyElement<'static> {
                    element! {
                        Text(
                            content: prefix_label.clone(),
                            color: PROMPT_PREFIX_FG,
                            weight: Weight::Bold,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                    .into()
                }))
                Textarea(
                    width: textarea_width,
                    min_height: 1u16,
                    max_height: Some(editor_max_height(props.screen_height)),
                    show_border: Some(false),
                    has_focus: has_focus,
                    prefix_config: Some(props.prefix_config),
                    input_prefix_kind: props.input_prefix_kind,
                    value: props.draft,
                    live_draft: props.live_draft,
                    suppress_enter_newline: props.suppress_enter_newline,
                    slash_palette_active: props.slash_palette_active,
                    force_palette_sync: props.force_palette_sync,
                    force_clear: props.force_clear,
                    submit_on_enter: true,
                    on_submit: props.on_submit.take(),
                    on_escape: props.on_escape.take(),
                    text_color: Some(text_color),
                    cursor_color: Some(EDITOR_CURSOR),
                )
            }
            View(
                position: Position::Absolute,
                right: 1,
                bottom: 0,
                margin_bottom: -1,
                background_color: Color::Reset,
            ) {
                Text(
                    color: label_color,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                    content: format!(" {} ", props.agent_mode.footer_label()),
                )
            }
            #(props
                .blocked_hint
                .as_ref()
                .filter(|text| !text.is_empty())
                .map(|hint| -> AnyElement<'static> {
                    element! {
                        View(
                            position: Position::Absolute,
                            left: 0,
                            top: 0,
                            width: textarea_width,
                            height: 1,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            background_color: Color::Reset,
                        ) {
                            Text(
                                content: hint.clone(),
                                color: EDITOR_TEXT_DIMMED,
                                wrap: TextWrap::NoWrap,
                            )
                        }
                    }
                    .into()
                }))
        }
    }
}

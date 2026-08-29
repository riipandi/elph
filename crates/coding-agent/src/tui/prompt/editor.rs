//! Multiline prompt editor with dynamic prefix and project label on the bottom border.

use elph_tui::PaletteKeyInput;
use elph_tui::Textarea;
use elph_tui::components::UiTheme;
use elph_tui::{InputPrefixKind, PREFIX_COLUMN_WIDTH, PromptPrefixConfig, effective_prefix_kind, prefix_symbol};
use iocraft::prelude::*;

use crate::types::AgentMode;

use crate::tui::chrome::fit_editor_border_project;
use crate::tui::theme::prompt_border_color;
use crate::tui::theme::{EDITOR_CURSOR, EDITOR_TEXT_DIMMED, EDITOR_TEXT_FOCUSED, PROMPT_PREFIX_FG};

fn editor_max_height(screen_height: u16) -> u16 {
    // Reserve rows for header, status, footer, borders, and a thin transcript band
    // so the footer stays on-screen even when the terminal is short.
    const RESERVED_CHROME_ROWS: u16 = 7;
    let budget = screen_height.saturating_sub(RESERVED_CHROME_ROWS).max(1);
    let preferred = (screen_height / 4).clamp(1, 12);
    preferred.min(budget)
}

#[derive(Default, Props)]
pub struct EditorProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub has_focus: bool,
    /// Project folder name for the bottom-border badge (`~ name (branch)`).
    pub project_name: String,
    /// Git branch for the bottom-border badge (parentheses form).
    pub git_branch: Option<String>,
    /// Bumped with chrome refresh so the border project label repaints.
    pub chrome_revision: u64,
    pub prefix_config: PromptPrefixConfig,
    pub input_prefix_kind: Option<Ref<InputPrefixKind>>,
    pub draft: Option<State<String>>,
    pub live_draft: Option<Ref<String>>,
    pub suppress_enter_newline: Option<Ref<bool>>,
    pub slash_palette_active: Option<Ref<bool>>,
    pub file_picker_active: Option<Ref<bool>>,
    pub styled_content: Option<Ref<String>>,
    pub live_cursor: Option<Ref<usize>>,
    pub force_palette_sync: Option<Ref<bool>>,
    pub force_clear: Option<Ref<bool>>,
    pub on_submit: HandlerMut<'static, String>,
    pub on_escape: HandlerMut<'static, ()>,
    pub on_file_picker_key: HandlerMut<'static, PaletteKeyInput>,
    pub file_picker_key_handled: Option<Ref<bool>>,
    pub prompt_editor_mirror: Option<Ref<(String, usize)>>,
    /// Selection yank toast (`y`) — shell drains into ephemeral banner.
    pub clipboard_toast: Option<State<Option<elph_tui::ClipboardNotice>>>,
    /// Directory for staged clipboard images.
    pub image_attachment_dir: Option<std::path::PathBuf>,
    /// Pending clipboard images paired with `[Image #N]` markers.
    pub image_attachments: Option<Ref<Vec<elph_tui::ImageAttachment>>>,
    /// Whether the active model accepts image input.
    pub supports_images: bool,
    /// Shown centered when the editor is blocked by an inline dialog.
    pub blocked_hint: Option<String>,
    /// Native terminal text-select mode (mouse capture off). Prompt stays focused/interactive;
    /// footer shows a `sel` badge. Kept for chrome parity (no longer dims or steals focus).
    pub text_select_mode: bool,
}

#[component]
pub fn Editor(props: &mut EditorProps) -> impl Into<AnyElement<'static>> {
    let _chrome_revision = props.chrome_revision;
    let theme = UiTheme::default();
    let _text_select_mode = props.text_select_mode;
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
    // Sit on the bottom border before the corner: leave room for `╯` and spacing.
    let border_label_max = (props.screen_width as usize)
        .saturating_sub(6)
        .min((props.screen_width as usize * 3) / 5)
        .max(1);
    let border_project = fit_editor_border_project(&props.project_name, props.git_branch.as_deref(), border_label_max);
    let border_badge = if border_project.is_empty() {
        String::new()
    } else {
        format!(" {border_project} ")
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
                    file_picker_active: props.file_picker_active,
                    styled_content: props.styled_content,
                    live_cursor: props.live_cursor,
                    force_palette_sync: props.force_palette_sync,
                    force_clear: props.force_clear,
                    clipboard_toast: props.clipboard_toast,
                    image_attachment_dir: props.image_attachment_dir.clone(),
                    image_attachments: props.image_attachments,
                    supports_images: props.supports_images,
                    submit_on_enter: true,
                    on_submit: props.on_submit.take(),
                    on_escape: props.on_escape.take(),
                    on_file_picker_key: props.on_file_picker_key.take(),
                    file_picker_key_handled: props.file_picker_key_handled,
                    prompt_editor_mirror: props.prompt_editor_mirror,
                    text_color: Some(text_color),
                    cursor_color: Some(EDITOR_CURSOR),
                )
            }
            #( (!border_badge.is_empty()).then(|| -> AnyElement<'static> {
                element! {
                    View(
                        position: Position::Absolute,
                        right: 1,
                        bottom: 0,
                        margin_bottom: -1,
                        background_color: Color::Reset,
                    ) {
                        Text(
                            color: Color::DarkGrey,
                            wrap: TextWrap::NoWrap,
                            content: border_badge.clone(),
                        )
                    }
                }
                .into()
            }))
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

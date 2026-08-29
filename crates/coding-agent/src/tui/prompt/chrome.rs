//! Editor + footer column (bottom chrome).

use elph_tui::PaletteKeyInput;
use elph_tui::components::UiTheme;
use elph_tui::{ImageAttachment, InputPrefixKind, image_marker_id_at_cursor};
use iocraft::prelude::*;

use crate::tui::labels::GitFooterInfo;
use crate::types::{AgentMode, ThinkingLevel};

use super::editor::Editor;
use super::footer::Footer;
use crate::tui::file_picker::{FilePickerPalette, FilePickerSnapshot};
use crate::tui::prompt_history::{PromptHistoryPalette, PromptHistorySnapshot};
use crate::tui::slash_palette::palette_anchor_bottom;
use crate::tui::slash_palette::{SlashCommandPalette, SlashPaletteSnapshot};

fn render_image_preview_dialog(
    attachment: &ImageAttachment,
    width: u16,
    bottom: u16,
    theme: UiTheme,
) -> AnyElement<'static> {
    let filename = attachment
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("attachment.png");
    element! {
        View(
            width: width,
            position: Position::Absolute,
            left: 0,
            bottom: bottom,
            flex_shrink: 0f32,
            align_items: AlignItems::FlexStart,
        ) {
            View(
                width: width,
                padding_left: theme.padding_md,
                padding_right: theme.padding_md,
                padding_top: theme.padding_sm,
                padding_bottom: theme.padding_sm,
                flex_direction: FlexDirection::Column,
                border_style: BorderStyle::Round,
                border_color: theme.accent,
                background_color: theme.surface,
            ) {
                Text(
                    content: format!("Image #{}", attachment.id),
                    color: theme.accent,
                    weight: Weight::Bold,
                    wrap: TextWrap::NoWrap,
                )
                Text(
                    content: format!(
                        "Format: PNG\nDimensions: {} × {}\nFile: {filename}",
                        attachment.width, attachment.height
                    ),
                    color: theme.text_primary,
                    wrap: TextWrap::Wrap,
                )
                Text(
                    content: "Move the cursor away to close preview",
                    color: theme.text_muted,
                    wrap: TextWrap::NoWrap,
                )
            }
        }
    }
    .into()
}

#[derive(Default, Props)]
pub struct PromptChromeProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub agent_mode: AgentMode,
    pub thinking_level: ThinkingLevel,
    pub has_focus: bool,
    pub project_name: String,
    pub git: Option<GitFooterInfo>,
    pub turn: u32,
    pub model_label: String,
    pub supports_images: bool,
    /// When false, footer accents use dimmed grey only.
    pub colored_status_footer: bool,
    pub chrome_revision: u64,
    pub draft: Option<State<String>>,
    pub live_draft: Option<Ref<String>>,
    pub input_prefix_kind: Option<Ref<InputPrefixKind>>,
    pub suppress_enter_newline: Option<Ref<bool>>,
    pub slash_palette_active: Option<Ref<bool>>,
    pub file_picker_active: Option<Ref<bool>>,
    pub styled_content: Option<Ref<String>>,
    pub live_cursor: Option<Ref<usize>>,
    pub force_palette_sync: Option<Ref<bool>>,
    pub force_editor_clear: Option<Ref<bool>>,
    pub slash_palette_snapshot: SlashPaletteSnapshot,
    pub slash_palette_selected: Option<State<usize>>,
    pub file_picker_snapshot: FilePickerSnapshot,
    pub file_picker_selected: Option<State<usize>>,
    pub file_picker_show_hidden: bool,
    pub prompt_history_snapshot: PromptHistorySnapshot,
    pub prompt_history_selected: Option<State<usize>>,
    /// Inline dialog anchored above the editor (e.g. model picker); same slot as slash palette.
    pub editor_overlay: Option<AnyElement<'static>>,
    pub on_submit: HandlerMut<'static, String>,
    pub on_escape: HandlerMut<'static, ()>,
    pub on_file_picker_key: HandlerMut<'static, PaletteKeyInput>,
    pub file_picker_key_handled: Option<Ref<bool>>,
    pub prompt_editor_mirror: Option<Ref<(String, usize)>>,
    pub clipboard_toast: Option<State<Option<elph_tui::ClipboardNotice>>>,
    pub image_attachment_dir: Option<std::path::PathBuf>,
    pub image_attachments: Option<Ref<Vec<elph_tui::ImageAttachment>>>,
    pub blocked_hint: Option<String>,
    /// Native terminal text-select mode (mouse capture off). Prompt stays interactive.
    pub text_select_mode: bool,
    /// Live multi-worker count for footer badge (≥2 shows `⬡ N`).
    pub worker_live_count: usize,
    /// This process worker memorable name (footer when multi-worker).
    pub worker_name: String,
    /// Pending inbound worker messages not yet seen (>0 colors `⬡` yellow).
    pub worker_pending_count: usize,
    /// True while the agent is replying to / sending a response for a peer (colors `⬡` green).
    pub worker_replying: bool,
    /// When true, hide the editor + palettes (e.g. while the `/aside` panel owns the
    /// bottom band). The footer below stays visible.
    pub hide_editor: bool,
}

#[component]
pub fn PromptChrome(props: &mut PromptChromeProps) -> impl Into<AnyElement<'static>> {
    let draft_text = props
        .live_draft
        .as_ref()
        .map(|live| live.read().clone())
        .or_else(|| props.draft.as_ref().map(|draft| draft.read().clone()))
        .unwrap_or_default();
    let palette_anchor = palette_anchor_bottom(&draft_text, props.screen_width, props.screen_height);
    let image_preview = if props.has_focus
        && !props.hide_editor
        && !props.slash_palette_snapshot.visible
        && !props.file_picker_snapshot.visible
        && !props.prompt_history_snapshot.visible
        && props.editor_overlay.is_none()
    {
        let cursor = props
            .live_cursor
            .as_ref()
            .map(|cursor| cursor.get())
            .unwrap_or(draft_text.len());
        image_marker_id_at_cursor(&draft_text, cursor).and_then(|(_, _, id)| {
            props.image_attachments.and_then(|attachments| {
                attachments
                    .read()
                    .iter()
                    .find(|attachment| attachment.id == id)
                    .cloned()
            })
        })
    } else {
        None
    };
    let image_preview_view = image_preview.as_ref().map(|attachment| {
        render_image_preview_dialog(attachment, props.screen_width, palette_anchor, UiTheme::default())
    });

    // Editor + palettes are dropped while an overlay (e.g. `/aside`) owns the bottom
    // band; the footer below always renders. Build the subtree first so we can skip it.
    let editor_view: Option<AnyElement<'static>> = if props.hide_editor {
        None
    } else {
        Some(
            element! {
                View(
                    width: props.screen_width,
                    flex_shrink: 0f32,
                    position: Position::Relative,
                    align_items: AlignItems::FlexStart,
                ) {
                    Editor(
                        screen_width: props.screen_width,
                        screen_height: props.screen_height,
                        agent_mode: props.agent_mode,
                        has_focus: props.has_focus,
                        project_name: props.project_name.clone(),
                        git_branch: props.git.as_ref().map(|g| g.branch.clone()),
                        chrome_revision: props.chrome_revision,
                        input_prefix_kind: props.input_prefix_kind,
                        draft: props.draft,
                        live_draft: props.live_draft,
                        suppress_enter_newline: props.suppress_enter_newline,
                        slash_palette_active: props.slash_palette_active,
                        file_picker_active: props.file_picker_active,
                        styled_content: props.styled_content,
                        live_cursor: props.live_cursor,
                        force_palette_sync: props.force_palette_sync,
                        force_clear: props.force_editor_clear,
                        blocked_hint: props.blocked_hint.clone(),
                        text_select_mode: props.text_select_mode,
                        clipboard_toast: props.clipboard_toast,
                        image_attachment_dir: props.image_attachment_dir.clone(),
                        image_attachments: props.image_attachments,
                        supports_images: props.supports_images,
                        on_submit: props.on_submit.take(),
                        on_escape: if props.slash_palette_snapshot.visible
                            || props.file_picker_snapshot.visible
                            || props.prompt_history_snapshot.visible
                        {
                            HandlerMut::default()
                        } else {
                            props.on_escape.take()
                        },
                        on_file_picker_key: props.on_file_picker_key.take(),
                        file_picker_key_handled: props.file_picker_key_handled,
                        prompt_editor_mirror: props.prompt_editor_mirror,
                    )
                    SlashCommandPalette(
                        screen_width: props.screen_width,
                        screen_height: props.screen_height,
                        agent_mode: props.agent_mode,
                        snapshot: props.slash_palette_snapshot.clone(),
                        anchor_bottom: palette_anchor,
                        selected_index: props.slash_palette_selected,
                    )
                    FilePickerPalette(
                        screen_width: props.screen_width,
                        screen_height: props.screen_height,
                        agent_mode: props.agent_mode,
                        snapshot: props.file_picker_snapshot.clone(),
                        anchor_bottom: palette_anchor,
                        selected_index: props.file_picker_selected,
                        show_hidden_files: props.file_picker_show_hidden,
                    )
                    PromptHistoryPalette(
                        screen_width: props.screen_width,
                        screen_height: props.screen_height,
                        agent_mode: props.agent_mode,
                        snapshot: props.prompt_history_snapshot.clone(),
                        anchor_bottom: palette_anchor,
                        selected_index: props.prompt_history_selected,
                    )
                    #(props.editor_overlay.take().map(|overlay| -> AnyElement<'static> {
                        element! {
                            View(
                                width: props.screen_width,
                                position: Position::Absolute,
                                left: 0,
                                bottom: palette_anchor,
                                flex_shrink: 0f32,
                                align_items: AlignItems::FlexStart,
                            ) {
                                #(overlay)
                            }
                        }
                        .into()
                    }))
                    #(image_preview_view)
                }
            }
            .into(),
        )
    };

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            border_style: BorderStyle::None,
            align_items: AlignItems::FlexStart,
            flex_direction: FlexDirection::Column,
            margin_bottom: 0,
            padding_top: 0,
            padding_bottom: 0,
            padding_left: 0,
            padding_right: 0,
        ) {
            #(editor_view)
            Footer(
                screen_width: props.screen_width,
                agent_mode: props.agent_mode,
                model_label: props.model_label.clone(),
                thinking_level: props.thinking_level,
                supports_images: props.supports_images,
                turn: props.turn,
                git: props.git.clone(),
                colored_status_footer: props.colored_status_footer,
                select_mode: props.text_select_mode,
                worker_live_count: props.worker_live_count,
                worker_name: props.worker_name.clone(),
                chrome_revision: props.chrome_revision,
                worker_pending_count: props.worker_pending_count,
                worker_replying: props.worker_replying,
            )
        }
    }
}

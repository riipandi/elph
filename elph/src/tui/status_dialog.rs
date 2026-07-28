//! Tool-approval dialog below the status row (ask-user prompts use [`super::user_question_bar`] above).

use elph_tui::components::{
    DialogUserInputContent, SELECT_LIST_AUTO_HEIGHT, SelectList, UiTheme, select_list_total_rows,
};
use iocraft::prelude::*;

use crate::tui::chrome::StatusRow;
use crate::tui::inline_dialog::{InlineDialogShell, OPTIONS_LIST_TOP_GAP, inline_body_width};
use crate::tui::provider_connect_dialog::{PendingProviderApiKeyDialog, ProviderConnectFocus, ProviderConnectStep};
use crate::tui::tool_approval::{
    PendingModeChange, PendingToolApproval, feedback_footer_hint, feedback_select_options, mode_change_footer_hint,
    mode_change_select_options, plan_confirmation_footer_hint, plan_confirmation_select_options,
    tool_approval_footer_hint, tool_approval_select_options,
};
use crate::tui::tool_params::{format_tool_approval_summary, tool_approval_summary_row_count_for_summary};

/// Max rows shown for the approval summary before the list.
const TOOL_PARAMS_MAX_VIEWPORT: u16 = 2;

/// Minimum rows reserved for parameters when space is tight.
const TOOL_PARAMS_MIN_VIEWPORT: u16 = 2;

/// Layout budget for the tool-approval inline dialog body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToolApprovalLayoutPlan {
    args_viewport: Option<u16>,
    list_height: u16,
}

/// Max inner body rows for the inline approval dialog (avoids double-counting shell chrome).
fn tool_approval_max_body_rows(screen_height: u16) -> u16 {
    let prompt_floor = (screen_height / 4).clamp(4, 12).saturating_add(1);
    let reserved = 1u16
        .saturating_add(4)
        .saturating_add(prompt_floor)
        .saturating_add(3)
        .saturating_add(1);
    screen_height.saturating_sub(reserved).max(4)
}

fn tool_approval_layout_plan(
    _screen_width: u16,
    screen_height: u16,
    summary: &str,
    body_width: u16,
) -> ToolApprovalLayoutPlan {
    let theme = UiTheme::default();
    let max_body = tool_approval_max_body_rows(screen_height);
    let options = tool_approval_select_options();
    let list_rows = select_list_total_rows(&options, false, body_width, theme, true) as u16;
    let has_args = !summary.is_empty();
    let args_rows = if has_args {
        tool_approval_summary_row_count_for_summary(summary, body_width)
    } else {
        0
    };
    let args_block = if has_args { args_rows } else { 0 };
    let natural_body = args_block
        .saturating_add(OPTIONS_LIST_TOP_GAP)
        .saturating_add(list_rows);

    if natural_body <= max_body {
        return ToolApprovalLayoutPlan {
            args_viewport: None,
            list_height: SELECT_LIST_AUTO_HEIGHT,
        };
    }

    let mut list_height = list_rows.min(max_body).max(4);
    let mut args_cap = max_body
        .saturating_sub(list_height)
        .saturating_sub(OPTIONS_LIST_TOP_GAP);

    let args_viewport = if !has_args || args_rows <= args_cap {
        None
    } else {
        let min_args = TOOL_PARAMS_MIN_VIEWPORT.min(args_rows).min(TOOL_PARAMS_MAX_VIEWPORT);
        if list_height
            .saturating_add(OPTIONS_LIST_TOP_GAP)
            .saturating_add(min_args)
            > max_body
        {
            list_height = max_body
                .saturating_sub(OPTIONS_LIST_TOP_GAP)
                .saturating_sub(min_args)
                .max(4)
                .min(list_rows);
            args_cap = max_body
                .saturating_sub(list_height)
                .saturating_sub(OPTIONS_LIST_TOP_GAP);
        }
        Some(args_cap.clamp(1, TOOL_PARAMS_MAX_VIEWPORT))
    };

    ToolApprovalLayoutPlan {
        args_viewport,
        list_height,
    }
}

fn render_tool_approval_dialog(
    props: &mut StatusZoneProps,
    tool_name: &str,
    args_summary: &str,
) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);
    let summary = format_tool_approval_summary(tool_name, args_summary);
    let plan = tool_approval_layout_plan(props.screen_width, props.screen_height, &summary, body_width);
    let options = tool_approval_select_options();
    let has_args = !summary.is_empty();

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: format!("Allow tool: {tool_name}"),
            has_focus: props.approval_has_focus,
            footer_hint: Some(tool_approval_footer_hint()),
        ) {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 0,
                flex_shrink: 0f32,
            ) {
                #(if has_args {
                    let summary_viewport =
                        plan.args_viewport
                            .unwrap_or(tool_approval_summary_row_count_for_summary(&summary, body_width));
                    Some(element! {
                        View(
                            width: body_width,
                            height: summary_viewport,
                            overflow: Overflow::Hidden,
                            flex_shrink: 0f32,
                        ) {
                            Text(
                                content: summary,
                                color: theme.text_secondary,
                                wrap: TextWrap::Wrap,
                            )
                        }
                    })
                } else {
                    None
                })
                View(
                    width: body_width,
                    padding_top: OPTIONS_LIST_TOP_GAP,
                    flex_shrink: 0f32,
                ) {
                    SelectList(
                        width: body_width,
                        height: plan.list_height,
                        options: options,
                        selected_index: props.approval_selected,
                        has_focus: props.approval_has_focus,
                        show_description: false,
                        compact: true,
                        theme: Some(theme),
                    )
                }
            }
        }
    }
    .into()
}

fn render_plan_confirmation_dialog(props: &mut StatusZoneProps, plan_text: &str) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);
    let options = plan_confirmation_select_options();
    // Show first 8 lines of the plan, truncated per line.
    let plan_preview: String = plan_text
        .lines()
        .take(8)
        .map(|line| elph_tui::utils::truncate_with_ellipsis(line, body_width as usize))
        .collect::<Vec<_>>()
        .join("\n");

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: "Plan confirmation".to_string(),
            has_focus: props.approval_has_focus,
            footer_hint: Some(plan_confirmation_footer_hint()),
        ) {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 1,
                flex_shrink: 0f32,
            ) {
                View(
                    width: body_width,
                    flex_shrink: 0f32,
                    overflow: Overflow::Hidden,
                ) {
                    Text(
                        content: plan_preview,
                        color: theme.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                }
                View(
                    width: body_width,
                    padding_top: 1,
                    flex_shrink: 0f32,
                ) {
                    SelectList(
                        width: body_width,
                        height: 5u16,
                        options: options,
                        selected_index: props.approval_selected,
                        has_focus: props.approval_has_focus,
                        show_description: false,
                        compact: true,
                        theme: Some(theme),
                    )
                }
            }
        }
    }
    .into()
}

fn render_mode_change_dialog(props: &mut StatusZoneProps, target_mode: &str, reason: &str) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);
    let mode_label = target_mode.to_ascii_uppercase();
    let options = mode_change_select_options();

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: format!("Switch to {mode_label} mode?"),
            has_focus: props.approval_has_focus,
            footer_hint: Some(mode_change_footer_hint()),
        ) {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                gap: 1,
                flex_shrink: 0f32,
            ) {
                View(
                    width: body_width,
                    flex_shrink: 0f32,
                    overflow: Overflow::Hidden,
                ) {
                    Text(
                        content: reason.to_string(),
                        color: theme.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                }
                View(
                    width: body_width,
                    padding_top: 1,
                    flex_shrink: 0f32,
                ) {
                    SelectList(
                        width: body_width,
                        height: 4u16,
                        options: options,
                        selected_index: props.approval_selected,
                        has_focus: props.approval_has_focus,
                        show_description: false,
                        compact: true,
                        theme: Some(theme),
                    )
                }
            }
        }
    }
    .into()
}

/// Which action is highlighted on the selected queue row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptQueueAction {
    #[default]
    SendNow,
    Edit,
    Cancel,
}

impl PromptQueueAction {
    pub const ALL: [Self; 3] = [Self::SendNow, Self::Edit, Self::Cancel];

    pub fn label(self) -> &'static str {
        match self {
            Self::SendNow => "Send",
            Self::Edit => "Edit",
            Self::Cancel => "Cancel",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::SendNow => Self::Edit,
            Self::Edit => Self::Cancel,
            Self::Cancel => Self::SendNow,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::SendNow => Self::Cancel,
            Self::Edit => Self::SendNow,
            Self::Cancel => Self::Edit,
        }
    }
}

/// Dialogs in the status zone (tool approval below StatusRow; prompt queue above it).
#[derive(Debug, Clone)]
pub enum StatusDialogKind {
    ToolApproval {
        tool_name: String,
        args_summary: String,
    },
    /// Mode-change approval dialog (Switch to Build/Brave).
    ModeChange {
        target_mode: String,
        reason: String,
    },
    /// Plan confirmation dialog (Implement / Implement fresh / Stay in Plan).
    PlanConfirmation {
        #[allow(dead_code)]
        plan_id: String,
        plan_text: String,
    },
    /// Feedback dialog (Report a Bug / Join Community).
    Feedback,
    /// Provider connection dialog with OAuth or API key input.
    ProviderConnect {
        provider_id: Option<String>,
        step: ProviderConnectStep,
        input_focus: ProviderConnectFocus,
    },
    /// Dedicated API key input dialog (separate from provider selection).
    ProviderApiKey {
        provider_id: String,
        provider_name: String,
    },
    /// Numbered prompt queue — rendered **above** StatusRow.
    PromptQueue {
        items: Vec<crate::agent::QueuedPromptItem>,
        selected: usize,
        /// Highlighted action on the selected row (Ctrl+Q manager).
        action: PromptQueueAction,
        /// When true, show action selection affordance on the focused row.
        interactive: bool,
    },
}

/// Props for [`StatusZone`] — optional fixed toast, status row, tool-approval / queue dialog.
///
/// Spinner/elapsed tick inside [`StatusRow`]; pass wall-clock start instants only.
#[derive(Props)]
pub struct StatusZoneProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub busy: bool,
    pub activity_label: String,
    pub accent: Color,
    pub activity_started_at: Option<std::time::Instant>,
    pub busy_started_at: Option<std::time::Instant>,
    pub session_elapsed_secs: f64,
    pub idle_notice: Option<String>,
    /// Fixed toast above the status row (agent mode, quit-busy, …).
    pub ephemeral_banner: Option<(String, Color)>,
    pub quit_confirm_pending: bool,
    /// Sticky StatusRow chrome while mouse capture is off for native text selection.
    pub select_mode: bool,
    pub dialog: Option<StatusDialogKind>,
    pub approval_selected: Option<State<usize>>,
    pub approval_has_focus: bool,
    pub api_key_input: Option<State<String>>,
    /// Provider connect dialog selection state.
    pub provider_connect_selected: Option<State<usize>>,
    /// Provider connect dialog filter text.
    pub provider_connect_filter: Option<State<String>>,
    /// Provider connect dialog input focus.
    pub provider_connect_input_focus: Option<State<ProviderConnectFocus>>,
    /// Queued prompt count for StatusRow badge (independent of manager open).
    pub queue_count: u32,
    /// Mouse click on `[Send]` / `[Edit]` / `[Cancel]` — `(display_index, action)`.
    ///
    /// Uses clonable [`Handler`] so each chip can bind its own `(index, action)`.
    pub on_queue_action: Handler<(usize, PromptQueueAction)>,
}

impl Default for StatusZoneProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            busy: false,
            activity_label: String::new(),
            accent: Color::White,
            activity_started_at: None,
            busy_started_at: None,
            session_elapsed_secs: 0.0,
            idle_notice: None,
            ephemeral_banner: None,
            quit_confirm_pending: false,
            select_mode: false,
            dialog: None,
            approval_selected: None,
            approval_has_focus: false,
            api_key_input: None,
            provider_connect_selected: None,
            provider_connect_filter: None,
            provider_connect_input_focus: None,
            queue_count: 0,
            on_queue_action: Handler::default(),
        }
    }
}

/// One-line toast pinned above the status row (outside the transcript scroll).
fn render_ephemeral_banner(screen_width: u16, text: &str, color: Color) -> AnyElement<'static> {
    let max_w = screen_width.saturating_sub(2).max(1) as usize;
    let content = elph_tui::utils::truncate_with_ellipsis(text, max_w);
    element! {
        View(
            width: screen_width,
            height: 1,
            flex_shrink: 0f32,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Start,
            padding_left: 1,
            padding_right: 1,
            // Breathing room before StatusRow (banner text was flush against tips/activity).
            margin_bottom: 1,
        ) {
            Text(
                color: color,
                wrap: TextWrap::NoWrap,
                content: content,
            )
        }
    }
    .into()
}

#[component]
pub fn StatusZone(props: &mut StatusZoneProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let on_queue_action = props.on_queue_action.clone();
    // Prompt queue sits above StatusRow (same vertical band as UserQuestionBar / permission UI).
    // Tool approval stays below StatusRow (existing layout).
    let above_status_row = match props.dialog.clone() {
        Some(StatusDialogKind::PromptQueue {
            items,
            selected,
            action,
            interactive,
        }) => Some(render_prompt_queue_dialog(
            props.screen_width,
            &items,
            selected,
            action,
            interactive,
            on_queue_action,
        )),
        _ => None,
    };
    let below_status_row = match props.dialog.clone() {
        Some(StatusDialogKind::ToolApproval {
            tool_name,
            args_summary,
        }) => Some(render_tool_approval_dialog(props, &tool_name, &args_summary)),
        Some(StatusDialogKind::ModeChange { target_mode, reason }) => {
            Some(render_mode_change_dialog(props, &target_mode, &reason))
        }
        Some(StatusDialogKind::PlanConfirmation { plan_text, .. }) => {
            Some(render_plan_confirmation_dialog(props, &plan_text))
        }
        Some(StatusDialogKind::Feedback) => Some(render_feedback_dialog(props)),
        Some(StatusDialogKind::ProviderConnect {
            provider_id,
            step,
            input_focus,
        }) => Some(render_provider_connect_dialog(props, provider_id, step, input_focus)),
        Some(StatusDialogKind::ProviderApiKey {
            provider_id,
            provider_name,
        }) => Some(render_provider_api_key_dialog(props, &provider_id, &provider_name)),
        _ => None,
    };
    let banner = props
        .ephemeral_banner
        .as_ref()
        .map(|(text, color)| render_ephemeral_banner(props.screen_width, text, *color));

    /// Render the provider connect dialog.
    fn render_provider_connect_dialog(
        props: &mut StatusZoneProps,
        _provider_id: Option<String>,
        step: ProviderConnectStep,
        input_focus: ProviderConnectFocus,
    ) -> AnyElement<'static> {
        use crate::tui::provider_connect_dialog::render_provider_connect_dialog as render_dialog;

        let selected = props
            .provider_connect_selected
            .clone()
            .or_else(|| props.approval_selected.clone())
            .expect("provider_connect_selected or approval_selected should be set");
        let api_key_input = props.api_key_input.clone().expect("api_key_input should be set");
        let filter = props
            .provider_connect_filter
            .clone()
            .expect("provider_connect_filter should be set");

        render_dialog(
            props.screen_width,
            props.approval_has_focus,
            selected,
            filter,
            api_key_input,
            step,
            input_focus,
        )
    }

    /// Render the dedicated API key input dialog.
    fn render_provider_api_key_dialog(
        props: &mut StatusZoneProps,
        _provider_id: &str,
        provider_name: &str,
    ) -> AnyElement<'static> {
        let theme = UiTheme::default();
        let body_width = inline_body_width(props.screen_width);
        let api_key_input = props.api_key_input.clone().expect("api_key_input should be set");

        element! {
            InlineDialogShell(
                screen_width: props.screen_width,
                title: format!("API Key \u{2014} {}", provider_name),
                has_focus: props.approval_has_focus,
                footer_hint: Some(format!("Enter confirm \u{00B7} Esc cancel \u{00B7} Provider: {}", provider_name)),
            ) {
                View(width: body_width, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                    View(width: body_width, flex_shrink: 0f32) {
                        Text(
                            content: format!("Enter your API key for {}:", provider_name),
                            color: theme.text_secondary,
                            wrap: TextWrap::Wrap,
                        )
                    }
                    View(width: body_width, padding_top: 1, flex_shrink: 0f32) {
                        DialogUserInputContent(
                            width: body_width,
                            placeholder: format!("sk-... ({} API key)", provider_name),
                            value: Some(api_key_input),
                            has_focus: props.approval_has_focus,
                            theme: Some(theme),
                            compact: true,
                            show_prompt: false,
                            show_footer_hint: false,
                            dialog_chrome: true,
                            on_submit: HandlerMut::default(),
                            on_cancel: HandlerMut::default(),
                        )
                    }
                }
            }
        }
        .into()
    }

    element! {
        View(
            width: props.screen_width,
            flex_shrink: 0f32,
            flex_direction: FlexDirection::Column,
        ) {
            #(banner)
            #(above_status_row)
            StatusRow(
                screen_width: props.screen_width,
                busy: props.busy,
                activity_label: props.activity_label.clone(),
                accent: props.accent,
                activity_started_at: props.activity_started_at,
                busy_started_at: props.busy_started_at,
                session_elapsed_secs: props.session_elapsed_secs,
                idle_notice: props.idle_notice.clone(),
                quit_confirm_pending: props.quit_confirm_pending,
                select_mode: props.select_mode,
                queue_count: props.queue_count,
            )
            #(below_status_row)
        }
    }
}

/// Build the active tool-approval dialog, if any.
pub fn build_status_dialog_kind(tool: Option<&PendingToolApproval>) -> Option<StatusDialogKind> {
    let pending = tool?;
    Some(StatusDialogKind::ToolApproval {
        tool_name: pending.tool_name.clone(),
        args_summary: pending.args_summary.clone(),
    })
}

/// Build the active mode-change dialog, if any.
pub fn build_mode_change_dialog_kind(pending: Option<&PendingModeChange>) -> Option<StatusDialogKind> {
    let pending = pending?;
    Some(StatusDialogKind::ModeChange {
        target_mode: pending.target_mode.clone(),
        reason: pending.reason.clone(),
    })
}

/// Build the active plan-confirmation dialog, if any.
pub fn build_plan_confirmation_dialog_kind(
    pending: Option<&crate::tui::tool_approval::PendingPlanConfirmation>,
) -> Option<StatusDialogKind> {
    let pending = pending?;
    Some(StatusDialogKind::PlanConfirmation {
        plan_id: pending.plan_id.clone(),
        plan_text: pending.plan_text.clone(),
    })
}

/// Build the feedback dialog when pending.
pub fn build_feedback_dialog_kind(active: bool) -> Option<StatusDialogKind> {
    if active { Some(StatusDialogKind::Feedback) } else { None }
}

/// Build the provider connect dialog when pending.
pub fn build_provider_connect_dialog_kind(
    provider_id: Option<String>,
    step: Option<ProviderConnectStep>,
    _selected: State<usize>,
    has_focus: bool,
    input_focus: ProviderConnectFocus,
) -> Option<StatusDialogKind> {
    if has_focus {
        Some(StatusDialogKind::ProviderConnect {
            provider_id,
            step: step.unwrap_or(ProviderConnectStep::SelectProvider),
            input_focus,
        })
    } else {
        None
    }
}

/// Build the dedicated API key input dialog when pending.
pub fn build_provider_api_key_dialog_kind(
    pending: Option<&PendingProviderApiKeyDialog>,
    has_focus: bool,
) -> Option<StatusDialogKind> {
    if has_focus {
        let pending = pending?;
        Some(StatusDialogKind::ProviderApiKey {
            provider_id: pending.provider_id.clone(),
            provider_name: pending.provider_name.clone(),
        })
    } else {
        None
    }
}

/// Render the feedback dialog with Report a Bug and Join Community options.
fn render_feedback_dialog(props: &mut StatusZoneProps) -> AnyElement<'static> {
    let theme = UiTheme::default();
    let body_width = inline_body_width(props.screen_width);
    let options = feedback_select_options();

    element! {
        InlineDialogShell(
            screen_width: props.screen_width,
            title: "Feedback".to_string(),
            has_focus: props.approval_has_focus,
            footer_hint: Some(feedback_footer_hint()),
        ) {
            View(
                width: body_width,
                flex_direction: FlexDirection::Column,
                flex_shrink: 0f32,
            ) {
                View(
                    width: body_width,
                    flex_shrink: 0f32,
                    overflow: Overflow::Hidden,
                ) {
                    Text(
                        content: "How can you help improve Elph?".to_string(),
                        color: theme.text_secondary,
                        wrap: TextWrap::Wrap,
                    )
                }
                View(
                    width: body_width,
                    padding_top: 1,
                    flex_shrink: 0f32,
                ) {
                    SelectList(
                        width: body_width,
                        height: 4u16,
                        options: options,
                        selected_index: props.approval_selected,
                        has_focus: props.approval_has_focus,
                        show_description: false,
                        compact: true,
                        theme: Some(theme),
                    )
                }
            }
        }
    }
    .into()
}

/// Build prompt-queue list when there are items (always visible above StatusRow while queued).
pub fn build_prompt_queue_dialog_kind(
    items: &[crate::agent::QueuedPromptItem],
    selected: usize,
    action: PromptQueueAction,
    interactive: bool,
) -> Option<StatusDialogKind> {
    if items.is_empty() {
        return None;
    }
    Some(StatusDialogKind::PromptQueue {
        items: items.to_vec(),
        selected: selected.min(items.len().saturating_sub(1)),
        action,
        interactive,
    })
}

/// Single-line queue rows: `#n title…` left, `[Send] [Edit] [Cancel]` right (timestamp-style rail).
fn render_prompt_queue_dialog(
    screen_width: u16,
    items: &[crate::agent::QueuedPromptItem],
    selected: usize,
    action: PromptQueueAction,
    interactive: bool,
    on_queue_action: Handler<(usize, PromptQueueAction)>,
) -> AnyElement<'static> {
    use crate::tui::theme::{EDITOR_TEXT_FOCUSED, PROMPT_QUEUE_FG};
    use elph_tui::utils::{display_width, truncate_with_ellipsis};

    const MAX_ITEMS: usize = 5;
    let inner_w = screen_width.saturating_sub(2).max(12);
    let selected = selected.min(items.len().saturating_sub(1));
    let start = if items.len() <= MAX_ITEMS {
        0
    } else {
        selected.saturating_sub(MAX_ITEMS / 2).min(items.len() - MAX_ITEMS)
    };
    let end = (start + MAX_ITEMS).min(items.len());
    let rows: Vec<AnyElement<'static>> = items[start..end]
        .iter()
        .enumerate()
        .map(|(offset, item)| {
            let idx = start + offset;
            let row_focused = idx == selected && interactive;
            let actions_w = queue_actions_rail_width() as u16;
            // "#1 " + title budget (gap before right rail).
            let num = format!("#{} ", item.seq);
            let num_w = display_width(&num) as u16;
            let title_budget = inner_w
                .saturating_sub(num_w)
                .saturating_sub(actions_w)
                .saturating_sub(1)
                .max(4) as usize;
            let one_line = item.text.lines().next().unwrap_or("").trim();
            let title = truncate_with_ellipsis(one_line, title_budget);
            let action_chips = render_queue_action_chips(action, row_focused, idx, &on_queue_action);
            element! {
                View(
                    width: screen_width,
                    height: 1,
                    flex_shrink: 0f32,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding_left: 1,
                    padding_right: 1,
                ) {
                    View(
                        flex_direction: FlexDirection::Row,
                        flex_grow: 1f32,
                        flex_shrink: 1f32,
                        overflow: Overflow::Hidden,
                        align_items: AlignItems::Center,
                    ) {
                        Text(color: PROMPT_QUEUE_FG, wrap: TextWrap::NoWrap, content: num)
                        Text(color: EDITOR_TEXT_FOCUSED, wrap: TextWrap::NoWrap, content: title)
                    }
                    View(
                        flex_direction: FlexDirection::Row,
                        flex_shrink: 0f32,
                        align_items: AlignItems::Center,
                        gap: 1,
                    ) {
                        #(action_chips)
                    }
                }
            }
            .into()
        })
        .collect();

    element! {
        View(
            width: screen_width,
            flex_shrink: 0f32,
            flex_direction: FlexDirection::Column,
        ) {
            #(rows)
            // One blank row under the list before StatusRow.
            View(width: screen_width, height: 1, flex_shrink: 0f32) {}
        }
    }
    .into()
}

fn queue_actions_rail_width() -> usize {
    // "[Send] [Edit] [Cancel]" with single spaces between chips.
    PromptQueueAction::ALL
        .iter()
        .map(|a| a.label().len() + 2) // [label]
        .sum::<usize>()
        + PromptQueueAction::ALL.len().saturating_sub(1)
}

fn render_queue_action_chips(
    selected: PromptQueueAction,
    row_focused: bool,
    row_index: usize,
    on_queue_action: &Handler<(usize, PromptQueueAction)>,
) -> Vec<AnyElement<'static>> {
    use crate::tui::theme::{PROMPT_QUEUE_FG, PROMPT_QUEUE_SELECTED_FG};
    PromptQueueAction::ALL
        .iter()
        .copied()
        .map(|a| {
            let label = format!("[{}]", a.label());
            let color = if row_focused && a == selected {
                PROMPT_QUEUE_SELECTED_FG
            } else {
                PROMPT_QUEUE_FG
            };
            // Clonable Handler + bind → each chip is an independent hit target.
            let click = on_queue_action.bind((row_index, a));
            element! {
                Button(
                    has_focus: false,
                    handler: click,
                ) {
                    Text(color: color, wrap: TextWrap::NoWrap, content: label)
                }
            }
            .into()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_args_json() -> String {
        let command = "cargo test ".to_string() + &"x".repeat(400);
        format!(r#"{{"command":"{command}","path":"src/main.rs","note":"{}"}}"#, "y".repeat(120))
    }

    #[test]
    fn layout_plan_keeps_summary_compact_on_short_screen() {
        let body_width = inline_body_width(80);
        let raw = long_args_json();
        let summary = format_tool_approval_summary("shell_exec", &raw);
        let summary_rows = tool_approval_summary_row_count_for_summary(&summary, body_width);
        let plan = tool_approval_layout_plan(80, 24, &summary, body_width);
        assert!(summary_rows <= TOOL_PARAMS_MAX_VIEWPORT);
        if let Some(viewport) = plan.args_viewport {
            assert!(viewport <= TOOL_PARAMS_MAX_VIEWPORT);
        }
    }

    #[test]
    fn layout_plan_grows_naturally_when_space_allows() {
        let body_width = inline_body_width(100);
        let summary = format_tool_approval_summary("read_file", r#"{"path":"src/lib.rs"}"#);
        let plan = tool_approval_layout_plan(100, 60, &summary, body_width);
        assert!(plan.args_viewport.is_none());
        assert_eq!(plan.list_height, SELECT_LIST_AUTO_HEIGHT);
    }

    #[test]
    fn layout_plan_keeps_approval_list_rows_reserved() {
        let body_width = inline_body_width(80);
        let theme = UiTheme::default();
        let options = tool_approval_select_options();
        let list_rows = select_list_total_rows(&options, false, body_width, theme, true) as u16;
        let summary = format_tool_approval_summary("shell_exec", &long_args_json());
        let plan = tool_approval_layout_plan(80, 24, &summary, body_width);
        assert!(plan.list_height == SELECT_LIST_AUTO_HEIGHT || plan.list_height >= list_rows.min(3));
    }
}

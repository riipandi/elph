//! Root shell: layout zones, global keyboard handling, and session state.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use elph_agent::{LocalExecutionEnv, PromptTemplate, Skill};
use elph_tui::components::{scroll_view_down, scroll_view_up};
use elph_tui::rgb;
use elph_tui::{
    InputPrefixKind, PromptPrefixConfig, absorb_inline_triggers, compose_palette_draft, resolve_submit_draft,
    try_consume_trigger,
};
use iocraft::prelude::*;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;

use crate::agent::slash_commands_for_palette;
use crate::agent::{AgentUiEvent, CodingAgentSession, ToolApprovalChoice};
use crate::extensions::ExtensionHost;
use crate::platform::exit_message::{ExitSnapshot, record_if_active};
use crate::platform::handle_prompt_interrupt_text;
use crate::platform::{Paths, PromptInterrupt, Settings};
use crate::types::{AgentMode, SlashCommand, ThinkingLevel};
use crate::types::{is_force_quit_command, is_quit_command};

use crate::tui::activity::TurnTokenTracker;
use crate::tui::activity::{
    accumulate_session_elapsed, activity_label_for_event, format_quit_canceled_notice, format_shell_canceled_notice,
    format_turn_canceled_notice, format_turn_complete_notice, user_shell_activity_label,
};
use crate::tui::agent_bridge::{PromptQueue, TranscriptEventApplier, TurnDispatcher};
use crate::tui::chrome::{ChromeStats, Header};
use crate::tui::chrome::{chrome_stats_from_session, format_elapsed_secs, read_git_footer_info, refresh_chrome_stats};
use crate::tui::focus::ShellFocus;
use crate::tui::focus::{is_text_select_toggle_key, prompt_focus_char, shell_global_shortcut};
use crate::tui::labels::GitFooterInfo;

use crate::tui::clipboard::copy_to_clipboard;
use crate::tui::confetti::{ConfettiOverlay, OpenConfettiArgs, PendingConfetti, close_confetti, open_confetti};
use crate::tui::file_picker::FilePickerKeyAction;
use crate::tui::file_picker::{
    FilePickerApplyContext, FilePickerSnapshot, active_mention_at_cursor, apply_file_picker_key,
    build_snapshot as build_file_picker_snapshot, file_picker_open, mention_highlight_ansi, mention_picker_visible,
    resolve_key_action as resolve_file_picker_key_action, sync_selection as sync_file_picker_selection,
};
use crate::tui::model_selector::ModelSelectorFocus;
use crate::tui::model_selector_bar::{ModelSelectorBar, ModelSelectorView};
use crate::tui::model_selector_shell::{
    OpenModelSelectorArgs, apply_model_selection_locally, apply_model_selector_filter_seed, close_model_selector,
    focus_model_selector_list, model_selector_confirm_on_enter, model_selector_filter_seed,
    model_selector_list_backspace, model_selector_list_nav_delta, model_selector_provider_delta,
    model_selector_sanitize_filter, model_selector_scope_delta, open_model_selector, spawn_runtime_model_switch,
    sync_pending_filter,
};
use crate::tui::prompt::PromptChrome;
use crate::tui::scoped_models::PendingScopedModels;
use crate::tui::scoped_models_bar::{ScopedModelsBar, ScopedModelsView};
use crate::tui::scoped_models_shell::{
    OpenScopedModelsArgs, apply_scoped_session, cancel_scoped_models, cycle_scoped_model_selection, open_scoped_models,
    save_scoped_models, scoped_models_list_nav_delta, scoped_models_reorder_delta, sync_scoped_filter,
};
use crate::tui::session_prefs::{cycle_and_persist_theme_mode, persist_session_prefs};
use crate::tui::shell_submit::{
    UserShellEvent, format_shell_agent_context, next_user_shell_tool_id, shell_exec_args_summary, spawn_user_shell,
};
use crate::tui::slash_handler::{SlashContext, SlashOutcome};
use crate::tui::slash_handler::{handle_slash_submit, overlay_deferred_message, slash_echoes_prompt_in_transcript};
use crate::tui::slash_palette::SlashPaletteKeyAction;
use crate::tui::slash_palette::{build_snapshot, palette_visible, resolve_snapshot_key_action, sync_selection};
use crate::tui::startup::{
    BootstrapPhase, BootstrapUiEvent, McpFooterLineKind, TuiBootstrapConfig, append_startup_warning,
    apply_mcp_server_progress, apply_mcp_startup_summary_line, begin_agent_startup, begin_mcp_startup,
    bootstrap_activity_label, bootstrap_is_active, classify_mcp_footer_line, mark_agent_startup_failed,
    mark_agent_startup_ready, mark_mcp_startup_failed, mcp_server_status_label, spawn_bootstrap_worker,
};
use crate::tui::status_dialog::{StatusZone, build_status_dialog_kind};
use crate::tui::system_prompt_dialog::{
    OpenSystemPromptDialogArgs, PendingSystemPromptDialog, SystemPromptDialogOverlay, close_system_prompt_dialog,
    open_system_prompt_dialog, system_prompt_dialog_chrome,
};
use crate::tui::tool_approval::PendingToolApproval;
use crate::tui::tool_approval::{
    TOOL_APPROVAL_DEFAULT_INDEX, choice_at_index, pick_tool_approval_index_from_key, tool_approval_transcript_key,
};
use crate::tui::tool_params::tool_display_verb;
use crate::tui::transcript::{
    EphemeralBanner, EphemeralBannerGeneration, QUIT_BUSY_NOTICE_KEY, TranscriptMessage, TranscriptPanel,
    TranscriptStyle, agent_mode_banner, agent_mode_busy_banner, api_error_banner, clear_ephemeral_banner,
    clear_ephemeral_banner_if_generation, expire_ephemeral_banner, prompt_copy_banner, prompt_copy_failed_banner,
    publish_ephemeral_banner, quit_busy_banner, select_mode_off_banner, select_mode_on_banner, theme_mode_banner,
    toggle_latest_collapsible_detail,
};
use crate::tui::user_question::PendingUserQuestion;
use crate::tui::user_question::{
    QuestionInputFocus, StepNavOutcome, advance_question_selection, apply_step_nav_outcome, apply_step_submit_outcome,
    current_choice_index, is_custom_choice_index, navigate_step_delta, pick_step_tab_from_key,
    question_option_nav_delta, question_step_nav_delta, reset_ui_for_step, select_value_at, snapshot_current_answer,
    step_activity_label, try_resolve_submittable_answer,
};
use crate::tui::user_question_bar::{UserQuestionBar, UserQuestionView};
use elph_agent::tools::fff_picker::MentionSearchIndex;
use elph_tui::PaletteKeyInput;
use elph_tui::components::ConfirmButtonFocus;

const SHELL_TICK_MS: u64 = 50;
const CHROME_REFRESH_TICKS: u32 = 20;
/// Base transcript publish interval while streaming (~10 Hz). Status spinner ticks in StatusRow.
const TRANSCRIPT_PUBLISH_MS: u64 = 100;
/// Faster transcript refresh while startup status lines are updating.
const STARTUP_TRANSCRIPT_PUBLISH_MS: u64 = 33;
/// Back off publish rate under heavy event bursts (CPU/memory headroom for input + scroll).
const TRANSCRIPT_PUBLISH_HEAVY_MS: u64 = 150;
const TRANSCRIPT_PUBLISH_BURST_MS: u64 = 180;
const MAX_UI_EVENTS_PER_TICK: usize = 48;
const MAX_BOOTSTRAP_EVENTS_PER_TICK: usize = 32;
/// How long the status row shows turn elapsed after completion before returning to tips.
const TURN_COMPLETE_NOTICE_MS: u64 = 5_000;
const FALLBACK_TERMINAL_WIDTH: u16 = 80;
const FALLBACK_TERMINAL_HEIGHT: u16 = 24;

fn initial_layout_screen_size() -> (u16, u16) {
    crossterm::terminal::size()
        .map(|(width, height)| (width.max(1), height.max(1)))
        .unwrap_or((FALLBACK_TERMINAL_WIDTH, FALLBACK_TERMINAL_HEIGHT))
}

fn merge_layout_screen_size(layout_size: &mut State<(u16, u16)>, hook_width: u16, hook_height: u16) {
    // Prefer a live terminal size. Taking max() with a stale larger value oversizes the
    // canvas and clips the footer off the bottom of the real terminal.
    let polled = crossterm::terminal::size()
        .ok()
        .map(|(width, height)| (width.max(1), height.max(1)));
    let from_hook = (hook_width > 0 && hook_height > 0).then_some((hook_width.max(1), hook_height.max(1)));
    let current = layout_size.get();
    let next = polled.or(from_hook).unwrap_or((current.0.max(1), current.1.max(1)));
    if next != current {
        layout_size.set(next);
    }
}

fn poll_layout_screen_size(layout_size: &mut State<(u16, u16)>) {
    if let Ok((width, height)) = crossterm::terminal::size() {
        let next = (width.max(1), height.max(1));
        if layout_size.get() != next {
            layout_size.set(next);
        }
    }
}

fn bump_chrome_ui_revision(chrome_ui_revision: &mut State<u64>) {
    chrome_ui_revision.set(chrome_ui_revision.get().wrapping_add(1));
}

/// Publish chrome stats when they change. Returns `true` if values were updated.
///
/// Callers that need a footer/header repaint even when values are unchanged
/// (AgentReady, first layout paint) should call `bump_chrome_ui_revision` when this returns `false`.
fn publish_chrome_stats(
    chrome_stats: &mut State<ChromeStats>,
    chrome_ui_revision: &mut State<u64>,
    stats: ChromeStats,
) -> bool {
    if *chrome_stats.read() == stats {
        return false;
    }
    chrome_stats.set(stats);
    bump_chrome_ui_revision(chrome_ui_revision);
    true
}

struct IdleStatusNotice {
    text: String,
    since: Instant,
}

fn count_submitted_user_prompts(messages: &[TranscriptMessage]) -> u32 {
    messages
        .iter()
        .filter(|message| {
            message.style.is_user_input_card() && message.submitted_at.is_some() && !message.content.trim().is_empty()
        })
        .count() as u32
}

fn live_turn_elapsed_secs(busy: bool, busy_started_at: &Option<Instant>) -> f64 {
    if !busy {
        return 0.0;
    }
    busy_started_at
        .as_ref()
        .map(|started| format_elapsed_secs(*started))
        .unwrap_or(0.0)
}

fn agent_event_keeps_busy(event: &AgentUiEvent) -> bool {
    matches!(
        event,
        AgentUiEvent::TextDelta(_)
            | AgentUiEvent::ThinkingDelta(_)
            | AgentUiEvent::ToolStart { .. }
            | AgentUiEvent::ToolUpdate { .. }
            | AgentUiEvent::ToolEnd { .. }
            | AgentUiEvent::SubagentStatus { .. }
    )
}

#[derive(Props)]
pub struct MainShellProps {
    pub session_id: String,
    pub startup_messages: Vec<TranscriptMessage>,
    pub bootstrap: Option<TuiBootstrapConfig>,
    pub initial_agent_mode: AgentMode,
    pub initial_thinking_level: ThinkingLevel,
    pub model_label: String,
    pub context_limit: u64,
    pub supports_images: bool,
    pub footer_token_display: String,
    pub colored_status_footer: bool,
    pub sticky_scroll: bool,
    pub show_thinking: bool,
    pub auto_expand_thinking: bool,
    pub agent_session: Option<Arc<CodingAgentSession>>,
    pub ui_events: Option<Arc<Mutex<UnboundedReceiver<AgentUiEvent>>>>,
    pub extension_host: ExtensionHost,
    pub slash_commands: Vec<SlashCommand>,
    pub prompt_templates: Vec<PromptTemplate>,
    pub skills: Vec<Skill>,
    pub cwd: PathBuf,
    pub execution_env: Arc<LocalExecutionEnv>,
    pub paths: Paths,
    pub file_picker_show_hidden: bool,
    pub initial_git_footer: Option<GitFooterInfo>,
}

impl Default for MainShellProps {
    fn default() -> Self {
        Self {
            session_id: "unavailable".to_string(),
            startup_messages: Vec::new(),
            bootstrap: None,
            initial_agent_mode: AgentMode::default(),
            initial_thinking_level: ThinkingLevel::default(),
            model_label: String::new(),
            context_limit: 200_000,
            supports_images: false,
            footer_token_display: "both".to_string(),
            colored_status_footer: true,
            sticky_scroll: false,
            show_thinking: false,
            auto_expand_thinking: false,
            agent_session: None,
            ui_events: None,
            extension_host: ExtensionHost::new(),
            slash_commands: Vec::new(),
            prompt_templates: Vec::new(),
            skills: Vec::new(),
            cwd: PathBuf::new(),
            execution_env: Arc::new(LocalExecutionEnv::new(".")),
            paths: Paths::resolve().expect("resolve elph paths"),
            file_picker_show_hidden: false,
            initial_git_footer: None,
        }
    }
}

struct BusyActivation<'a> {
    busy: &'a mut State<bool>,
    busy_started_at: &'a mut Ref<Option<Instant>>,
    activity_started_at: &'a mut Ref<Option<Instant>>,
    activity_label: &'a mut State<String>,
    last_activity_label: &'a mut Ref<String>,
}

fn mark_busy(ctx: &mut BusyActivation<'_>, steer: bool, activity_label: Option<&str>) {
    let now = Instant::now();
    let label = activity_label.map(str::to_string).unwrap_or_else(|| {
        if steer {
            "Steering".to_string()
        } else {
            "Thinking".to_string()
        }
    });
    ctx.busy.set(true);
    ctx.busy_started_at.set(Some(now));
    ctx.activity_started_at.set(Some(now));
    ctx.activity_label.set(label.clone());
    ctx.last_activity_label.set(label);
}

struct PendingQuitAction<'a> {
    pending_quit_confirm: &'a mut Ref<bool>,
    should_exit: &'a mut State<bool>,
    busy: &'a State<bool>,
    turn_cancel_requested: &'a mut Ref<bool>,
    prompt_queue: &'a mut Ref<PromptQueue>,
    pending_tool_approval: &'a mut Ref<Option<PendingToolApproval>>,
    pending_user_question: &'a mut Ref<Option<PendingUserQuestion>>,
    agent_session: &'a Option<Arc<CodingAgentSession>>,
}

/// Show a fixed toast above StatusRow. Timed banners schedule an async clear that does **not**
/// wait for agent busy/stream to finish; generation guards ignore stale clear tasks.
fn show_ephemeral_banner(
    ephemeral_banner: &mut State<Option<EphemeralBanner>>,
    generation: &mut Ref<EphemeralBannerGeneration>,
    expire_tx: &UnboundedSender<u64>,
    banner: EphemeralBanner,
) {
    let mut slot = ephemeral_banner.read().clone();
    let mut banner_gen = generation.get();
    let (id, ttl) = publish_ephemeral_banner(&mut slot, &mut banner_gen, banner);
    generation.set(banner_gen);
    ephemeral_banner.set(slot);
    if let Some(ttl) = ttl {
        let tx = expire_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            let _ = tx.send(id);
        });
    }
}

fn clear_quit_busy_banner(
    ephemeral_banner: &mut State<Option<EphemeralBanner>>,
    generation: &mut Ref<EphemeralBannerGeneration>,
) {
    let mut slot = ephemeral_banner.read().clone();
    if clear_ephemeral_banner(&mut slot, Some(QUIT_BUSY_NOTICE_KEY)) {
        // Invalidate pending async clears for previous timed notices.
        let mut banner_gen = generation.get();
        banner_gen.bump();
        generation.set(banner_gen);
        ephemeral_banner.set(slot);
    }
}

fn poll_ephemeral_banner_expiry(
    ephemeral_banner: &mut State<Option<EphemeralBanner>>,
    generation: &Ref<EphemeralBannerGeneration>,
    expire_rx: &mut UnboundedReceiver<u64>,
) {
    while let Ok(id) = expire_rx.try_recv() {
        let mut slot = ephemeral_banner.read().clone();
        let banner_gen = generation.get();
        if clear_ephemeral_banner_if_generation(&mut slot, &banner_gen, id) {
            ephemeral_banner.set(slot);
        }
    }
    // Wall-clock safety net (e.g. if a sleep task was dropped).
    let mut slot = ephemeral_banner.read().clone();
    if expire_ephemeral_banner(&mut slot) {
        ephemeral_banner.set(slot);
    }
}

fn arm_pending_quit(
    pending_quit_confirm: &mut Ref<bool>,
    ephemeral_banner: &mut State<Option<EphemeralBanner>>,
    generation: &mut Ref<EphemeralBannerGeneration>,
    expire_tx: &UnboundedSender<u64>,
) {
    if pending_quit_confirm.get() {
        return;
    }
    pending_quit_confirm.set(true);
    show_ephemeral_banner(ephemeral_banner, generation, expire_tx, quit_busy_banner());
}

fn dismiss_pending_quit(
    pending_quit_confirm: &mut Ref<bool>,
    idle_status_notice: &mut Ref<Option<IdleStatusNotice>>,
    ephemeral_banner: &mut State<Option<EphemeralBanner>>,
    generation: &mut Ref<EphemeralBannerGeneration>,
) {
    if !pending_quit_confirm.get() {
        return;
    }
    pending_quit_confirm.set(false);
    clear_quit_busy_banner(ephemeral_banner, generation);
    idle_status_notice.set(Some(IdleStatusNotice {
        text: format_quit_canceled_notice(),
        since: Instant::now(),
    }));
}

fn confirm_pending_quit(
    ctx: PendingQuitAction<'_>,
    ephemeral_banner: &mut State<Option<EphemeralBanner>>,
    generation: &mut Ref<EphemeralBannerGeneration>,
) {
    ctx.pending_quit_confirm.set(false);
    clear_quit_busy_banner(ephemeral_banner, generation);
    if ctx.busy.get() {
        ctx.turn_cancel_requested.set(true);
        ctx.prompt_queue.write().clear();
        if let Some(pending) = ctx.pending_tool_approval.write().take() {
            pending.respond(ToolApprovalChoice::Reject);
        }
        if let Some(question) = ctx.pending_user_question.write().take() {
            question.respond(String::new());
        }
        if let Some(session) = ctx.agent_session.as_ref() {
            TurnDispatcher::spawn_abort(Arc::clone(session));
        }
    }
    ctx.should_exit.set(true);
}

/// Request application exit. Returns `true` when the shell should exit now.
fn request_quit(
    ctx: PendingQuitAction<'_>,
    ephemeral_banner: &mut State<Option<EphemeralBanner>>,
    generation: &mut Ref<EphemeralBannerGeneration>,
    expire_tx: &UnboundedSender<u64>,
    force: bool,
) -> bool {
    if force {
        confirm_pending_quit(ctx, ephemeral_banner, generation);
        return true;
    }
    if ctx.busy.get() {
        if ctx.pending_quit_confirm.get() {
            confirm_pending_quit(ctx, ephemeral_banner, generation);
            true
        } else {
            arm_pending_quit(ctx.pending_quit_confirm, ephemeral_banner, generation, expire_tx);
            false
        }
    } else {
        ctx.pending_quit_confirm.set(false);
        ctx.should_exit.set(true);
        true
    }
}

fn begin_turn_token_tracking(tracker: &mut Ref<Option<TurnTokenTracker>>, chrome: &ChromeStats) {
    tracker.set(Some(TurnTokenTracker::new(chrome.tokens_used)));
}

fn push_transcript_message(
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
    message: TranscriptMessage,
) {
    messages.set({
        let mut list = messages.read().clone();
        list.push(message);
        list
    });
    messages_revision.set(messages_revision.get().wrapping_add(1));
}

fn publish_transcript_now(
    messages_revision: &mut State<u64>,
    transcript_pending: &mut Ref<bool>,
    last_transcript_publish: &mut Ref<Instant>,
) {
    messages_revision.set(messages_revision.get().wrapping_add(1));
    transcript_pending.set(false);
    last_transcript_publish.set(Instant::now());
}

/// Adaptive publish interval: slower under large event bursts to keep UI input responsive.
fn transcript_publish_interval_ms(bootstrap_active: bool, event_burst: usize) -> u64 {
    if bootstrap_active {
        return STARTUP_TRANSCRIPT_PUBLISH_MS;
    }
    if event_burst >= 32 {
        TRANSCRIPT_PUBLISH_BURST_MS
    } else if event_burst >= 16 {
        TRANSCRIPT_PUBLISH_HEAVY_MS
    } else {
        TRANSCRIPT_PUBLISH_MS
    }
}

#[expect(clippy::too_many_arguments)]
fn apply_bootstrap_ui_event(
    event: BootstrapUiEvent,
    bootstrap_phase: &mut Ref<BootstrapPhase>,
    busy: &mut State<bool>,
    activity_label: &mut State<String>,
    activity_started_at: &mut Ref<Option<Instant>>,
    live_session_id: &mut State<String>,
    chrome_refresh_pending: &mut State<bool>,
    chrome_stats: &mut State<ChromeStats>,
    chrome_ui_revision: &mut State<u64>,
    fallback_context_limit: u64,
    palette_refresh_pending: &mut State<bool>,
    agent_session_slot: &mut Ref<Option<Arc<CodingAgentSession>>>,
    ui_events_slot: &mut Ref<Option<Arc<Mutex<UnboundedReceiver<AgentUiEvent>>>>>,
    messages: &mut State<Vec<TranscriptMessage>>,
) {
    match event {
        BootstrapUiEvent::AgentReady(bootstrap) => {
            live_session_id.set(bootstrap.session_id.clone());
            chrome_refresh_pending.set(true);
            // Always repaint chrome on AgentReady — stats may equal the bootstrap snapshot
            // (same model/context), but the footer must still show eagerly without waiting for
            // the first turn or a manual model pick.
            if !publish_chrome_stats(
                chrome_stats,
                chrome_ui_revision,
                chrome_stats_from_session(bootstrap.session.as_ref(), fallback_context_limit),
            ) {
                bump_chrome_ui_revision(chrome_ui_revision);
            }
            agent_session_slot.set(Some(Arc::clone(&bootstrap.session)));
            ui_events_slot.set(Some(Arc::clone(&bootstrap.ui_rx)));
            {
                let mut msgs = messages.write();
                let provider = bootstrap.session.model_provider();
                let model = bootstrap.session.model_id();
                mark_agent_startup_ready(
                    &mut msgs,
                    (!provider.trim().is_empty()).then_some(provider),
                    (!model.trim().is_empty()).then_some(model),
                );
            }
            bootstrap_phase.set(BootstrapPhase::AgentReady);
            activity_label.set(bootstrap_activity_label(BootstrapPhase::AgentReady, None));
        }
        BootstrapUiEvent::AgentFailed(err) => {
            log::warn!("agent bootstrap failed: {err}");
            bootstrap_phase.set(BootstrapPhase::Failed);
            busy.set(false);
            activity_label.set(bootstrap_activity_label(BootstrapPhase::Failed, None));
            {
                let mut msgs = messages.write();
                mark_agent_startup_failed(&mut msgs, &err);
                append_startup_warning(&mut msgs, "Run `elph doctor` or check logs.");
            }
        }
        BootstrapUiEvent::McpHeader { enabled_servers } => {
            bootstrap_phase.set(BootstrapPhase::McpLoading);
            activity_label.set(bootstrap_activity_label(BootstrapPhase::McpLoading, None));
            {
                let mut msgs = messages.write();
                begin_mcp_startup(&mut msgs, enabled_servers);
            }
        }
        BootstrapUiEvent::McpServer(progress) => {
            activity_label.set(mcp_server_status_label(&progress));
            activity_started_at.set(Some(Instant::now()));
            {
                let mut msgs = messages.write();
                apply_mcp_server_progress(&mut msgs, &progress);
            }
        }
        BootstrapUiEvent::McpTranscriptLine(line) => {
            let mut msgs = messages.write();
            match classify_mcp_footer_line(&line) {
                McpFooterLineKind::Summary(summary) => apply_mcp_startup_summary_line(&mut msgs, &summary),
                McpFooterLineKind::Warning(warning) => append_startup_warning(&mut msgs, &warning),
            }
        }
        BootstrapUiEvent::McpComplete => {
            bootstrap_phase.set(BootstrapPhase::Done);
            busy.set(false);
            activity_label.set(String::new());
            chrome_refresh_pending.set(true);
            palette_refresh_pending.set(true);
        }
        BootstrapUiEvent::McpFailed(err) => {
            log::warn!("MCP bootstrap failed: {err}");
            {
                let mut msgs = messages.write();
                mark_mcp_startup_failed(&mut msgs, &err);
            }
            bootstrap_phase.set(BootstrapPhase::Done);
            busy.set(false);
            activity_label.set(String::new());
        }
    }
}

#[component]
pub fn MainShell(props: &mut MainShellProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (hook_screen_width, hook_screen_height) = hooks.use_terminal_size();
    let mut layout_screen_size = hooks.use_state(initial_layout_screen_size);
    merge_layout_screen_size(&mut layout_screen_size, hook_screen_width, hook_screen_height);
    let (screen_width, screen_height) = layout_screen_size.get();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut should_exit = hooks.use_state(|| false);
    // When true, mouse capture is off so the terminal can native-select transcript text.
    let mut select_mode = hooks.use_state(|| false);
    // Apply every frame; iocraft only reconfigures the terminal when the value changes.
    system.set_mouse_capture(!select_mode.get());
    let mut agent_mode = hooks.use_state(|| props.initial_agent_mode);
    let mut thinking_level = hooks.use_state(|| props.initial_thinking_level);
    let mut draft = hooks.use_state(String::new);
    let mut live_draft = hooks.use_ref(String::new);
    let mut input_prefix_kind = hooks.use_ref(InputPrefixKind::default);
    let startup_messages = props.startup_messages.clone();
    let mut messages = hooks.use_state(move || startup_messages);
    let mut messages_revision = hooks.use_state(|| 0u64);
    let mut suppress_enter_newline = hooks.use_ref(|| false);
    let mut slash_palette_active = hooks.use_ref(|| false);
    let mut file_picker_active = hooks.use_ref(|| false);
    let mut file_picker_suppressed = hooks.use_ref(|| false);
    let mut file_picker_key_handled = hooks.use_ref(|| false);
    let mut force_palette_sync = hooks.use_ref(|| false);
    let mut force_editor_clear = hooks.use_ref(|| false);
    let mut busy = hooks.use_state(|| false);
    let mut activity_label = hooks.use_state(|| "Thinking".to_string());
    let mut session_elapsed_secs = hooks.use_state(|| 0.0f64);
    let session_wall_started_at = hooks.use_ref(Instant::now);
    let show_thinking = props.show_thinking;
    let mut busy_started_at = hooks.use_ref(|| None::<Instant>);
    let mut activity_started_at = hooks.use_ref(|| None::<Instant>);
    let mut last_activity_label = hooks.use_ref(String::new);
    let mut prompt_queue = hooks.use_ref(PromptQueue::default);
    let mut event_applier =
        hooks.use_ref(|| TranscriptEventApplier::new(props.show_thinking, props.auto_expand_thinking));
    let mut pending_tool_approval = hooks.use_ref(|| None::<PendingToolApproval>);
    let mut pending_user_question = hooks.use_ref(|| None::<PendingUserQuestion>);
    let mut slash_commands = hooks.use_state(|| props.slash_commands.clone());
    let mut prompt_templates = hooks.use_state(|| props.prompt_templates.clone());
    let mut skills = hooks.use_state(|| props.skills.clone());
    let mut slash_palette_index = hooks.use_state(|| 0usize);
    let mut slash_palette_query = hooks.use_ref(String::new);
    let mut file_picker_index = hooks.use_state(|| 0usize);
    let mut file_picker_query = hooks.use_ref(String::new);
    let mut live_cursor = hooks.use_ref(|| 0usize);
    let prompt_editor_mirror = hooks.use_ref(|| (String::new(), 0usize));
    let mut styled_content = hooks.use_ref(String::new);
    let mut mention_index = hooks.use_ref(|| None::<Arc<MentionSearchIndex>>);
    let mut mention_index_requested = hooks.use_ref(|| false);
    let mut file_picker_show_hidden = hooks.use_state(|| props.file_picker_show_hidden);
    let mut palette_refresh_pending = hooks.use_state(|| false);
    let mut shell_focus = hooks.use_state(ShellFocus::default);
    let mut question_selected = hooks.use_state(|| 0usize);
    let mut question_confirm_focus = hooks.use_state(ConfirmButtonFocus::default);
    let mut question_answer = hooks.use_state(String::new);
    let mut question_multi_checked = hooks.use_state(Vec::<bool>::new);
    let mut question_input_focus = hooks.use_state(QuestionInputFocus::default);
    let mut question_validation_error = hooks.use_state(|| None::<String>);
    let mut approval_selected = hooks.use_state(|| 0usize);
    let mut pending_model_selector = hooks.use_ref(|| None::<crate::tui::model_selector::PendingModelSelector>);
    let mut model_provider_index = hooks.use_state(|| 0usize);
    let mut model_selected_index = hooks.use_state(|| 0usize);
    let mut model_filter = hooks.use_state(String::new);
    let mut model_input_focus = hooks.use_state(ModelSelectorFocus::default);
    let mut pending_scoped_models = hooks.use_ref(|| None::<PendingScopedModels>);
    let mut scoped_selected_index = hooks.use_state(|| 0usize);
    let mut scoped_filter = hooks.use_state(String::new);
    let mut session_scoped_items = hooks.use_ref(|| {
        Settings::load(&props.paths)
            .map(|s| s.models.scoped)
            .unwrap_or_default()
    });
    let mut pending_system_prompt = hooks.use_ref(|| None::<PendingSystemPromptDialog>);
    let system_prompt_scroll = hooks.use_ref_default::<ScrollViewHandle>();
    let mut system_prompt_scroll_tick = hooks.use_ref(|| 0u32);
    let mut pending_confetti = hooks.use_ref(|| None::<PendingConfetti>);
    let mut confetti_runtime = hooks.use_ref(|| None::<crate::tui::confetti::ConfettiRuntime>);
    let mut confetti_frame = hooks.use_ref(|| 0u32);

    let extension_host = props.extension_host.clone();
    let cwd = props.cwd.clone();

    let mut agent_session_slot = hooks.use_ref(|| props.agent_session.clone());
    let mut ui_events_slot = hooks.use_ref(|| props.ui_events.clone());
    let mut bootstrap_phase = hooks.use_ref(|| {
        if props.bootstrap.is_some() {
            BootstrapPhase::Pending
        } else {
            BootstrapPhase::Done
        }
    });
    let bootstrap_config = hooks.use_ref(|| props.bootstrap.clone());
    let mut bootstrap_worker_started = hooks.use_ref(|| false);
    let mut bootstrap_rx = hooks.use_ref(|| None::<UnboundedReceiver<BootstrapUiEvent>>);
    let mut live_session_id = hooks.use_state(|| props.session_id.clone());
    let extension_host_for_palette = extension_host.clone();
    let execution_env = props.execution_env.clone();
    struct UserShellChannel {
        tx: UnboundedSender<UserShellEvent>,
        rx: UnboundedReceiver<UserShellEvent>,
    }
    let mut user_shell_channel = hooks.use_ref(|| {
        let (tx, rx) = unbounded_channel();
        UserShellChannel { tx, rx }
    });
    let mut user_shell_abort = hooks.use_ref(|| None::<CancellationToken>);
    let paths = hooks.use_state(|| props.paths.clone());
    let mut skills_count = hooks.use_state(|| 0usize);
    let mut chrome_refresh_pending = hooks.use_state(|| true);
    let mut chrome_stats = hooks.use_state(|| ChromeStats {
        context_limit: props.context_limit,
        model_label: props.model_label.clone(),
        supports_images: props.supports_images,
        ..ChromeStats::default()
    });
    let mut git_footer = hooks.use_state(|| props.initial_git_footer.clone());
    // Start at 1 so the first Footer paint depends on chrome_revision (iocraft child identity).
    let mut chrome_ui_revision = hooks.use_state(|| 1u64);
    let mut chrome_tick = hooks.use_ref(|| 0u32);
    // Ensures one forced chrome repaint after the first shell tick (layout size settled).
    let mut chrome_eager_paint_done = hooks.use_ref(|| false);
    let fallback_context_limit = props.context_limit;
    let fallback_model_label = props.model_label.clone();
    let fallback_model_label_for_chrome = fallback_model_label.clone();
    let fallback_supports_images = props.supports_images;
    let footer_token_display = props.footer_token_display.clone();
    let colored_status_footer = props.colored_status_footer;
    let session_id = live_session_id.read().clone();
    let mut transcript_pending = hooks.use_ref(|| false);
    let mut last_transcript_publish = hooks.use_ref(|| Instant::now() - Duration::from_millis(TRANSCRIPT_PUBLISH_MS));
    let mut last_event_burst = hooks.use_ref(|| 0usize);
    let mut idle_status_notice = hooks.use_ref(|| None::<IdleStatusNotice>);
    let mut turn_cancel_requested = hooks.use_ref(|| false);
    let mut pending_quit_confirm = hooks.use_ref(|| false);
    let mut turn_token_tracker = hooks.use_ref(|| None::<TurnTokenTracker>);
    // Fixed toast above status row (agent mode, quit-busy) — not in the scrollable transcript.
    // State (not Ref) so set/clear repaints without waiting for agent busy/stream updates.
    let mut ephemeral_banner = hooks.use_state(|| None::<EphemeralBanner>);
    let mut ephemeral_banner_generation = hooks.use_ref(EphemeralBannerGeneration::default);
    struct EphemeralExpireChannel {
        tx: UnboundedSender<u64>,
        rx: UnboundedReceiver<u64>,
    }
    let mut ephemeral_expire = hooks.use_ref(|| {
        let (tx, rx) = unbounded_channel();
        EphemeralExpireChannel { tx, rx }
    });

    let cwd_for_mention_index = cwd.clone();
    let mut layout_screen_size_for_loop = layout_screen_size;
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(SHELL_TICK_MS)).await;

            poll_layout_screen_size(&mut layout_screen_size_for_loop);

            if bootstrap_phase.get() == BootstrapPhase::Pending && !bootstrap_worker_started.get() {
                if let Some(config) = bootstrap_config.read().clone() {
                    bootstrap_worker_started.set(true);
                    let paths_snapshot = paths.read().clone();
                    bootstrap_rx.set(Some(spawn_bootstrap_worker(config, paths_snapshot)));
                    bootstrap_phase.set(BootstrapPhase::Running);
                    busy.set(true);
                    activity_started_at.set(Some(Instant::now()));
                    activity_label.set(bootstrap_activity_label(BootstrapPhase::Running, Some("Preparing agent")));
                    {
                        let mut msgs = messages.write();
                        begin_agent_startup(&mut msgs);
                    }
                    publish_transcript_now(
                        &mut messages_revision,
                        &mut transcript_pending,
                        &mut last_transcript_publish,
                    );
                } else {
                    bootstrap_phase.set(BootstrapPhase::Done);
                }
            }

            if let Some(rx) = bootstrap_rx.write().as_mut() {
                let mut bootstrap_events = 0usize;
                while bootstrap_events < MAX_BOOTSTRAP_EVENTS_PER_TICK {
                    let Ok(event) = rx.try_recv() else {
                        break;
                    };
                    bootstrap_events += 1;
                    apply_bootstrap_ui_event(
                        event,
                        &mut bootstrap_phase,
                        &mut busy,
                        &mut activity_label,
                        &mut activity_started_at,
                        &mut live_session_id,
                        &mut chrome_refresh_pending,
                        &mut chrome_stats,
                        &mut chrome_ui_revision,
                        fallback_context_limit,
                        &mut palette_refresh_pending,
                        &mut agent_session_slot,
                        &mut ui_events_slot,
                        &mut messages,
                    );
                    publish_transcript_now(
                        &mut messages_revision,
                        &mut transcript_pending,
                        &mut last_transcript_publish,
                    );
                }
            }

            let agent_session_for_loop = agent_session_slot.read().clone();
            let agent_session_for_chrome = agent_session_slot.read().clone();
            let agent_session_for_palette = agent_session_slot.read().clone();
            let ui_events = ui_events_slot.read().clone();

            if mention_index_requested.get() && mention_index.read().is_none() {
                let base = cwd_for_mention_index.to_string_lossy().into_owned();
                if let Ok(Ok(index)) = tokio::task::spawn_blocking(move || MentionSearchIndex::build(&base)).await {
                    mention_index.set(Some(Arc::new(index)));
                }
            }

            if palette_refresh_pending.get() {
                if let Some(session) = agent_session_for_palette.as_ref() {
                    let resources = session.harness().get_resources().await;
                    let templates = resources.prompt_templates.clone();
                    let loaded_skills = resources.skills.clone();
                    prompt_templates.set(templates.clone());
                    skills.set(loaded_skills.clone());
                    slash_commands.set(slash_commands_for_palette(
                        Some(&extension_host_for_palette.registry().read()),
                        Some(&templates),
                        Some(&loaded_skills),
                    ));
                }
                palette_refresh_pending.set(false);
            }

            chrome_tick.set(chrome_tick.get().wrapping_add(1));
            let chrome_due = chrome_refresh_pending.get() || chrome_tick.get() % CHROME_REFRESH_TICKS == 0;
            if chrome_due {
                let paths = paths.read().clone();
                let next_git_footer = read_git_footer_info(paths.project_dir());
                if git_footer.read().clone() != next_git_footer {
                    git_footer.set(next_git_footer);
                    bump_chrome_ui_revision(&mut chrome_ui_revision);
                }

                if let Some(session) = agent_session_for_chrome.as_ref() {
                    chrome_refresh_pending.set(false);
                    let resources = session.harness().get_resources().await;
                    skills_count.set(resources.skills.len());
                    let stats = refresh_chrome_stats(
                        Arc::clone(session),
                        fallback_context_limit,
                        &fallback_model_label_for_chrome,
                        fallback_supports_images,
                    )
                    .await;
                    publish_chrome_stats(&mut chrome_stats, &mut chrome_ui_revision, stats.clone());
                    if busy.get()
                        && let Some(tracker) = turn_token_tracker.write().as_mut()
                    {
                        tracker.sync_baseline(stats.tokens_used);
                    }
                } else {
                    // No session yet: still finish the pending git/chrome snapshot so the
                    // bootstrap footer (project + model) paints without waiting for AgentReady.
                    // Previously pending stayed true forever and re-ran git I/O every tick.
                    chrome_refresh_pending.set(false);
                }

                // One-shot eager repaint after the first chrome pass (layout size is settled
                // and bootstrap labels are on the tree). Without this, iocraft can leave the
                // footer blank until the first stats mutation (model pick / first turn).
                if !chrome_eager_paint_done.get() {
                    chrome_eager_paint_done.set(true);
                    bump_chrome_ui_revision(&mut chrome_ui_revision);
                }
            }

            // Phase-timer reset only — spinner/elapsed animate inside StatusRow (no shell re-render).
            if busy.get() {
                let current_label = activity_label.read().clone();
                if current_label != *last_activity_label.read() {
                    last_activity_label.set(current_label);
                    activity_started_at.set(Some(Instant::now()));
                }
            }

            let idle_notice_expired = idle_status_notice
                .read()
                .as_ref()
                .is_some_and(|notice| notice.since.elapsed() >= Duration::from_millis(TURN_COMPLETE_NOTICE_MS));
            if idle_notice_expired {
                idle_status_notice.set(None);
            }

            if pending_confetti.read().is_some() {
                let (frame_changed, should_close) = {
                    if let Some(runtime) = confetti_runtime.write().as_mut() {
                        let frame_changed = runtime.tick();
                        (frame_changed, runtime.should_close())
                    } else {
                        (false, false)
                    }
                };
                if should_close {
                    close_confetti(
                        &mut pending_confetti,
                        &mut confetti_runtime,
                        &mut draft,
                        &mut live_draft,
                        &mut shell_focus,
                    );
                } else if frame_changed {
                    confetti_frame.set(confetti_frame.get().wrapping_add(1));
                }
            }

            {
                let mut channel = ephemeral_expire.write();
                poll_ephemeral_banner_expiry(&mut ephemeral_banner, &ephemeral_banner_generation, &mut channel.rx);
            }

            let mut transcript_changed = false;
            let mut run_completed = false;
            let mut run_completed_elapsed: Option<f64> = None;

            if let Some(rx) = ui_events.as_ref()
                && let Ok(mut guard) = rx.lock()
            {
                // Drain + coalesce stream deltas so one tick applies O(1) text/tool appends
                // instead of dozens of tiny mutations that each rebuild layout.
                let mut raw_events = Vec::with_capacity(MAX_UI_EVENTS_PER_TICK);
                while raw_events.len() < MAX_UI_EVENTS_PER_TICK {
                    let Ok(event) = guard.try_recv() else {
                        break;
                    };
                    raw_events.push(event);
                }
                last_event_burst.set(raw_events.len());
                let events = crate::tui::agent_bridge::coalesce_agent_ui_events(raw_events);
                for event in events {
                    if !busy.get() && agent_event_keeps_busy(&event) {
                        mark_busy(
                            &mut BusyActivation {
                                busy: &mut busy,
                                busy_started_at: &mut busy_started_at,
                                activity_started_at: &mut activity_started_at,
                                activity_label: &mut activity_label,
                                last_activity_label: &mut last_activity_label,
                            },
                            false,
                            None,
                        );
                    }
                    if let AgentUiEvent::RunCompleted { elapsed_secs } = &event {
                        run_completed = true;
                        run_completed_elapsed = Some(*elapsed_secs);
                    }

                    match &event {
                        AgentUiEvent::TextDelta(delta) => {
                            if let Some(tracker) = turn_token_tracker.write().as_mut() {
                                tracker.record_delta(delta);
                            }
                        }
                        AgentUiEvent::ThinkingDelta(delta) if show_thinking => {
                            if let Some(tracker) = turn_token_tracker.write().as_mut() {
                                tracker.record_delta(delta);
                            }
                        }
                        _ => {}
                    }

                    if let AgentUiEvent::Status(ref message) = event {
                        if message.to_ascii_lowercase().contains("reloaded") {
                            palette_refresh_pending.set(true);
                        }
                        // Sticky red toast — friendly text only (no raw JSON); transcript keeps fuller line.
                        if crate::tui::api_error_display::is_user_facing_api_error_line(message) {
                            let toast = crate::tui::api_error_display::format_ephemeral_api_error(message);
                            show_ephemeral_banner(
                                &mut ephemeral_banner,
                                &mut ephemeral_banner_generation,
                                &ephemeral_expire.read().tx,
                                api_error_banner(toast),
                            );
                        }
                    }

                    if let AgentUiEvent::ToolApprovalRequired(req) = event {
                        let tool_name = req.tool_name.clone();
                        let tool_call_id = req.tool_call_id.clone();
                        let verb = tool_display_verb(&tool_name);
                        activity_label.set(format!("Approve: {verb}"));
                        approval_selected.set(TOOL_APPROVAL_DEFAULT_INDEX);
                        shell_focus.set(ShellFocus::StatusDialog);
                        pending_tool_approval.set(Some(PendingToolApproval::from_request(req)));
                        {
                            let mut msgs = messages.write();
                            // Process status line (colored, consistent gaps) — not a flush Meta dump.
                            let key = tool_approval_transcript_key(&tool_call_id);
                            if let Some(existing) =
                                msgs.iter_mut().find(|m| m.startup_key.as_deref() == Some(key.as_str()))
                            {
                                existing.content = "Tool approval".to_string();
                                existing.status_detail = Some(verb.clone());
                                existing.style = TranscriptStyle::StatusRunning;
                            } else {
                                let mut row = TranscriptMessage::startup_status(
                                    key,
                                    "Tool approval".to_string(),
                                    TranscriptStyle::StatusRunning,
                                );
                                row.status_detail = Some(verb);
                                msgs.push(row);
                            }
                        }
                        transcript_changed = true;
                        continue;
                    }

                    if let AgentUiEvent::UserQuestionRequired(req) = event {
                        let pending = PendingUserQuestion::from_request(req);
                        activity_label.set(step_activity_label(&pending));
                        reset_ui_for_step(
                            &pending,
                            &mut question_selected,
                            &mut question_confirm_focus,
                            &mut question_answer,
                            &mut question_multi_checked,
                            &mut question_input_focus,
                        );
                        shell_focus.set(ShellFocus::StatusDialog);
                        pending_user_question.set(Some(pending));
                        transcript_changed = true;
                        continue;
                    }

                    if let Some(label) = activity_label_for_event(&event, show_thinking) {
                        activity_label.set(label);
                    }
                    {
                        let mut msgs = messages.write();
                        if event_applier.write().apply(&mut msgs, event) {
                            transcript_changed = true;
                        }
                    }
                }
            }

            while let Ok(event) = user_shell_channel.write().rx.try_recv() {
                match event {
                    UserShellEvent::ToolUpdate { id, chunk } => {
                        let mut msgs = messages.write();
                        if event_applier
                            .write()
                            .apply(&mut msgs, AgentUiEvent::ToolUpdate { id, output: chunk })
                        {
                            transcript_changed = true;
                        }
                    }
                    UserShellEvent::ToolEnd {
                        id,
                        exit_code,
                        output,
                        cancelled,
                        with_context,
                        command,
                    } => {
                        let is_error = !cancelled && exit_code != Some(0);
                        {
                            let mut msgs = messages.write();
                            if event_applier.write().apply(
                                &mut msgs,
                                AgentUiEvent::ToolEnd {
                                    id,
                                    is_error,
                                    output: output.clone(),
                                },
                            ) {
                                transcript_changed = true;
                            }
                        }
                        let shell_elapsed = busy_started_at
                            .read()
                            .as_ref()
                            .map(|started| format_elapsed_secs(*started))
                            .unwrap_or(0.0);
                        user_shell_abort.set(None);
                        turn_cancel_requested.set(false);
                        busy.set(false);
                        busy_started_at.set(None);
                        activity_started_at.set(None);
                        activity_label.set(String::new());
                        if cancelled {
                            idle_status_notice.set(Some(IdleStatusNotice {
                                text: format_shell_canceled_notice(shell_elapsed),
                                since: Instant::now(),
                            }));
                        }
                        if with_context
                            && !cancelled
                            && let Some(session) = agent_session_for_loop.as_ref()
                        {
                            let context = format_shell_agent_context(&command, &output);
                            TurnDispatcher::spawn_turn(Arc::clone(session), context, false);
                            mark_busy(
                                &mut BusyActivation {
                                    busy: &mut busy,
                                    busy_started_at: &mut busy_started_at,
                                    activity_started_at: &mut activity_started_at,
                                    activity_label: &mut activity_label,
                                    last_activity_label: &mut last_activity_label,
                                },
                                false,
                                None,
                            );
                        }
                    }
                }
            }

            if transcript_changed {
                transcript_pending.set(true);
            }

            let transcript_publish_ms =
                transcript_publish_interval_ms(bootstrap_is_active(bootstrap_phase.get()), last_event_burst.get());
            if transcript_pending.get()
                && (run_completed
                    || last_transcript_publish.get().elapsed().as_millis() >= transcript_publish_ms as u128)
            {
                messages_revision.set(messages_revision.get().wrapping_add(1));
                transcript_pending.set(false);
                last_transcript_publish.set(Instant::now());
            }

            if run_completed {
                pending_quit_confirm.set(false);
                clear_quit_busy_banner(&mut ephemeral_banner, &mut ephemeral_banner_generation);
                if let Some(turn_elapsed) = run_completed_elapsed {
                    session_elapsed_secs.set(accumulate_session_elapsed(session_elapsed_secs.get(), turn_elapsed));
                }
                busy.set(false);
                busy_started_at.set(None);
                activity_started_at.set(None);
                activity_label.set("Thinking".to_string());
                turn_token_tracker.set(None);
                chrome_refresh_pending.set(true);

                if let Some(next) = prompt_queue.write().pop_front() {
                    idle_status_notice.set(None);
                    turn_cancel_requested.set(false);
                    mark_busy(
                        &mut BusyActivation {
                            busy: &mut busy,
                            busy_started_at: &mut busy_started_at,
                            activity_started_at: &mut activity_started_at,
                            activity_label: &mut activity_label,
                            last_activity_label: &mut last_activity_label,
                        },
                        false,
                        None,
                    );
                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                    if let Some(session) = agent_session_for_loop.as_ref() {
                        chrome_refresh_pending.set(true);
                        TurnDispatcher::spawn_turn(Arc::clone(session), next, false);
                    }
                } else if turn_cancel_requested.get() {
                    turn_cancel_requested.set(false);
                    let elapsed = run_completed_elapsed.unwrap_or(0.0);
                    idle_status_notice.set(Some(IdleStatusNotice {
                        text: format_turn_canceled_notice(elapsed),
                        since: Instant::now(),
                    }));
                } else if let Some(elapsed_secs) = run_completed_elapsed {
                    idle_status_notice.set(Some(IdleStatusNotice {
                        text: format_turn_complete_notice(elapsed_secs),
                        since: Instant::now(),
                    }));
                }
            }
        }
    });

    let agent_session = agent_session_slot.read().clone();

    hooks.use_terminal_events({
        let paths = paths.read().clone();
        let agent_session = agent_session.clone();
        let extension_host_for_keys = extension_host.clone();
        let cwd_for_keys = cwd.clone();
        let mut messages = messages;
        let mut messages_revision = messages_revision;
        move |event| {
            let TerminalEvent::Key(KeyEvent {
                code, kind, modifiers, ..
            }) = event
            else {
                return;
            };
            if kind == KeyEventKind::Release {
                return;
            }

            // Textarea handles `@` picker keys before this hook; do not fall through to agent-mode Tab.
            if file_picker_key_handled.get() {
                file_picker_key_handled.set(false);
                return;
            }

            // Ctrl+S (or Ctrl+Shift+S) — toggle mouse capture for native text selection.
            // Persistent until toggled again. Skipped when scoped-models editor needs Ctrl+S to save
            // (that handler runs later while the overlay is open).
            let scoped_models_open_early = pending_scoped_models.read().is_some();
            if !scoped_models_open_early && is_text_select_toggle_key(modifiers, code) {
                let next = !select_mode.get();
                select_mode.set(next);
                let expire_tx = ephemeral_expire.read().tx.clone();
                show_ephemeral_banner(
                    &mut ephemeral_banner,
                    &mut ephemeral_banner_generation,
                    &expire_tx,
                    if next {
                        select_mode_on_banner()
                    } else {
                        select_mode_off_banner()
                    },
                );
                return;
            }

            let mut pending_tool_approval = pending_tool_approval;
            let mut pending_user_question = pending_user_question;
            let mut pending_model_selector = pending_model_selector;
            let mut model_provider_index = model_provider_index;
            let mut model_selected_index = model_selected_index;
            let mut model_filter = model_filter;
            let mut model_input_focus = model_input_focus;
            let mut question_multi_checked = question_multi_checked;
            let mut question_input_focus = question_input_focus;
            let mut question_validation_error = question_validation_error;
            let mut pending_quit_confirm = pending_quit_confirm;
            if pending_quit_confirm.get() && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                match code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        confirm_pending_quit(
                            PendingQuitAction {
                                pending_quit_confirm: &mut pending_quit_confirm,
                                should_exit: &mut should_exit,
                                busy: &busy,
                                turn_cancel_requested: &mut turn_cancel_requested,
                                prompt_queue: &mut prompt_queue,
                                pending_tool_approval: &mut pending_tool_approval,
                                pending_user_question: &mut pending_user_question,
                                agent_session: &agent_session,
                            },
                            &mut ephemeral_banner,
                            &mut ephemeral_banner_generation,
                        );
                        return;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        dismiss_pending_quit(
                            &mut pending_quit_confirm,
                            &mut idle_status_notice,
                            &mut ephemeral_banner,
                            &mut ephemeral_banner_generation,
                        );
                        return;
                    }
                    _ => {}
                }
            }

            let system_prompt_open = pending_system_prompt.read().is_some();
            let confetti_open = pending_confetti.read().is_some();
            let model_selector_open = pending_model_selector.read().is_some();
            let scoped_models_open = pending_scoped_models.read().is_some();
            let status_dialog_open = pending_tool_approval.read().is_some()
                || pending_user_question.read().is_some()
                || model_selector_open
                || scoped_models_open
                || system_prompt_open
                || confetti_open;

            if status_dialog_open {
                if confetti_open {
                    return;
                }

                if system_prompt_open {
                    let mut pending_system_prompt = pending_system_prompt;
                    let mut draft = draft;
                    let mut live_draft = live_draft;
                    let mut shell_focus = shell_focus;
                    let mut system_prompt_scroll = system_prompt_scroll;

                    if modifiers.is_empty() && code == KeyCode::Esc {
                        close_system_prompt_dialog(
                            &mut pending_system_prompt,
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                            &mut force_editor_clear,
                        );
                        return;
                    }

                    if modifiers.is_empty() {
                        match code {
                            KeyCode::Up | KeyCode::Char('k') => {
                                scroll_view_up(&mut system_prompt_scroll.write(), 1);
                                system_prompt_scroll_tick.set(system_prompt_scroll_tick.get().wrapping_add(1));
                                return;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                scroll_view_down(&mut system_prompt_scroll.write(), 1);
                                system_prompt_scroll_tick.set(system_prompt_scroll_tick.get().wrapping_add(1));
                                return;
                            }
                            KeyCode::PageUp => {
                                scroll_view_up(&mut system_prompt_scroll.write(), 10);
                                system_prompt_scroll_tick.set(system_prompt_scroll_tick.get().wrapping_add(1));
                                return;
                            }
                            KeyCode::PageDown => {
                                scroll_view_down(&mut system_prompt_scroll.write(), 10);
                                system_prompt_scroll_tick.set(system_prompt_scroll_tick.get().wrapping_add(1));
                                return;
                            }
                            _ => {}
                        }
                    }

                    if !shell_global_shortcut(modifiers, code) {
                        return;
                    }
                }

                if scoped_models_open
                    && pending_user_question.read().is_none()
                    && !system_prompt_open
                    && !confetti_open
                    && !model_selector_open
                {
                    let mut pending_scoped_models = pending_scoped_models;
                    let mut scoped_selected_index = scoped_selected_index;
                    let scoped_filter = scoped_filter;
                    let mut draft = draft;
                    let mut live_draft = live_draft;
                    let mut shell_focus = shell_focus;
                    let mut session_scoped_items = session_scoped_items;
                    let paths_snapshot = paths.clone();

                    if modifiers.is_empty() && code == KeyCode::Esc {
                        cancel_scoped_models(
                            &mut pending_scoped_models,
                            &mut session_scoped_items.write(),
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                        );
                        return;
                    }

                    // Scoped editor owns Ctrl+S for save (do not require !SHIFT — either chord saves).
                    if modifiers.contains(KeyModifiers::CONTROL)
                        && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::META)
                        && matches!(code, KeyCode::Char('s') | KeyCode::Char('S'))
                    {
                        if let Some(pending) = pending_scoped_models.write().as_mut() {
                            save_scoped_models(pending, &paths_snapshot, &mut session_scoped_items.write());
                            push_transcript_message(
                                &mut messages,
                                &mut messages_revision,
                                TranscriptMessage::text(
                                    format!("Scoped models saved ({} enabled).", pending.enabled_count()),
                                    TranscriptStyle::Meta,
                                ),
                            );
                        }
                        return;
                    }

                    if modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(code, KeyCode::Char('a') | KeyCode::Char('A'))
                    {
                        if let Some(pending) = pending_scoped_models.write().as_mut() {
                            sync_scoped_filter(pending, &scoped_filter.read());
                            pending.enable_all_visible_or_all();
                            apply_scoped_session(pending, &mut session_scoped_items.write());
                            scoped_selected_index.set(pending.selected_index);
                        }
                        return;
                    }

                    if modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(code, KeyCode::Char('x') | KeyCode::Char('X'))
                    {
                        if let Some(pending) = pending_scoped_models.write().as_mut() {
                            sync_scoped_filter(pending, &scoped_filter.read());
                            pending.clear_all_visible_or_all();
                            apply_scoped_session(pending, &mut session_scoped_items.write());
                            scoped_selected_index.set(pending.selected_index);
                        }
                        return;
                    }

                    if modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(code, KeyCode::Char('p') | KeyCode::Char('P'))
                    {
                        if let Some(pending) = pending_scoped_models.write().as_mut() {
                            sync_scoped_filter(pending, &scoped_filter.read());
                            pending.toggle_selected_provider();
                            apply_scoped_session(pending, &mut session_scoped_items.write());
                            scoped_selected_index.set(pending.selected_index);
                        }
                        return;
                    }

                    if let Some(delta) = scoped_models_reorder_delta(modifiers, code) {
                        if let Some(pending) = pending_scoped_models.write().as_mut() {
                            sync_scoped_filter(pending, &scoped_filter.read());
                            if pending.reorder_selected(delta) {
                                apply_scoped_session(pending, &mut session_scoped_items.write());
                                scoped_selected_index.set(pending.selected_index);
                            }
                        }
                        return;
                    }

                    if let Some(delta) = scoped_models_list_nav_delta(modifiers, code) {
                        if let Some(pending) = pending_scoped_models.write().as_mut() {
                            sync_scoped_filter(pending, &scoped_filter.read());
                            pending.move_selection(delta);
                            scoped_selected_index.set(pending.selected_index);
                        }
                        return;
                    }

                    if modifiers.is_empty() && code == KeyCode::Enter {
                        if let Some(pending) = pending_scoped_models.write().as_mut() {
                            sync_scoped_filter(pending, &scoped_filter.read());
                            pending.toggle_selected();
                            apply_scoped_session(pending, &mut session_scoped_items.write());
                            scoped_selected_index.set(pending.selected_index);
                        }
                        return;
                    }

                    if !shell_global_shortcut(modifiers, code) {
                        return;
                    }
                }

                if model_selector_open
                    && pending_user_question.read().is_none()
                    && !system_prompt_open
                    && !confetti_open
                    && !scoped_models_open
                {
                    let mut pending_model_selector = pending_model_selector;
                    let mut model_provider_index = model_provider_index;
                    let mut model_selected_index = model_selected_index;
                    let mut model_filter = model_filter;
                    let mut model_input_focus = model_input_focus;
                    let mut draft = draft;
                    let mut live_draft = live_draft;
                    let mut shell_focus = shell_focus;
                    let mut chrome_stats = chrome_stats;
                    let mut chrome_refresh_pending = chrome_refresh_pending;

                    if modifiers.contains(KeyModifiers::CONTROL)
                        && matches!(code, KeyCode::Char('l') | KeyCode::Char('L'))
                    {
                        close_model_selector(
                            &mut pending_model_selector,
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                        );
                        return;
                    }

                    if modifiers.is_empty() && code == KeyCode::Esc {
                        close_model_selector(
                            &mut pending_model_selector,
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                        );
                        return;
                    }

                    if modifiers.is_empty() && code == KeyCode::Tab {
                        let next = match model_input_focus.get() {
                            ModelSelectorFocus::Search => ModelSelectorFocus::List,
                            ModelSelectorFocus::List => ModelSelectorFocus::Search,
                        };
                        model_input_focus.set(next);
                        if let Some(pending) = pending_model_selector.write().as_mut() {
                            pending.input_focus = next;
                        }
                        return;
                    }

                    if model_input_focus.get() == ModelSelectorFocus::List
                        && let Some(seed) = model_selector_filter_seed(modifiers, code)
                        && let Some(pending) = pending_model_selector.write().as_mut()
                    {
                        apply_model_selector_filter_seed(seed, &mut model_filter, &mut model_input_focus, pending);
                        model_selected_index.set(pending.model_index);
                        return;
                    }

                    if let Some(delta) = model_selector_scope_delta(modifiers, code) {
                        if let Some(pending) = pending_model_selector.write().as_mut() {
                            sync_pending_filter(pending, &model_filter.read());
                            pending.apply_scope_nav(delta);
                            model_provider_index.set(pending.provider_index);
                            model_selected_index.set(pending.model_index);
                            if model_input_focus.get() == ModelSelectorFocus::List {
                                focus_model_selector_list(&mut model_input_focus, pending);
                            }
                        }
                        return;
                    }

                    if model_input_focus.get() == ModelSelectorFocus::List {
                        if let Some(delta) = model_selector_provider_delta(modifiers, code) {
                            if let Some(pending) = pending_model_selector.write().as_mut() {
                                focus_model_selector_list(&mut model_input_focus, pending);
                                sync_pending_filter(pending, &model_filter.read());
                                pending.apply_horizontal_nav(delta);
                                model_provider_index.set(pending.provider_index);
                                model_selected_index.set(pending.model_index);
                            }
                            return;
                        }

                        if modifiers.is_empty()
                            && code == KeyCode::Backspace
                            && let Some(pending) = pending_model_selector.write().as_mut()
                            && model_selector_list_backspace(model_input_focus.get(), &mut model_filter, pending)
                        {
                            model_selected_index.set(pending.model_index);
                            return;
                        }

                        if let Some(delta) = model_selector_list_nav_delta(modifiers, code) {
                            if let Some(pending) = pending_model_selector.write().as_mut() {
                                focus_model_selector_list(&mut model_input_focus, pending);
                                sync_pending_filter(pending, &model_filter.read());
                                let len = pending.filtered_models().len();
                                if len > 0 {
                                    let next =
                                        (pending.model_index as isize + delta).clamp(0, len as isize - 1) as usize;
                                    pending.model_index = next;
                                    model_selected_index.set(next);
                                }
                            }
                            return;
                        }
                    }

                    if modifiers.is_empty()
                        && code == KeyCode::Enter
                        && model_selector_confirm_on_enter(model_input_focus.get())
                    {
                        let selection = pending_model_selector.read().as_ref().and_then(|pending| {
                            let mut pending = pending.clone();
                            sync_pending_filter(&mut pending, &model_filter.read());
                            pending.selected_model().map(|row| row.value)
                        });
                        if let Some(value) = selection {
                            let paths_snapshot = paths.clone();
                            let agent = agent_session.clone();
                            let mut stats = chrome_stats.read().clone();
                            match apply_model_selection_locally(&value, &paths_snapshot, &mut stats) {
                                Ok(label) => {
                                    publish_chrome_stats(&mut chrome_stats, &mut chrome_ui_revision, stats);
                                    chrome_refresh_pending.set(true);
                                    push_transcript_message(
                                        &mut messages,
                                        &mut messages_revision,
                                        TranscriptMessage::text(format!("Model set to {label}"), TranscriptStyle::Meta),
                                    );
                                    if let Some(session) = agent {
                                        spawn_runtime_model_switch(session, value);
                                    }
                                }
                                Err(err) => {
                                    push_transcript_message(
                                        &mut messages,
                                        &mut messages_revision,
                                        TranscriptMessage::text(format!("{err}"), TranscriptStyle::Meta),
                                    );
                                }
                            }
                        }
                        close_model_selector(
                            &mut pending_model_selector,
                            &mut draft,
                            &mut live_draft,
                            &mut shell_focus,
                        );
                        return;
                    }

                    if !shell_global_shortcut(modifiers, code) {
                        return;
                    }
                }

                if (model_selector_open || scoped_models_open || system_prompt_open || confetti_open)
                    && pending_user_question.read().is_none()
                {
                    return;
                }

                let step_tab_jump = {
                    let pending_ref = pending_user_question.read();
                    match pending_ref.as_ref() {
                        Some(pending) if pending.step_count() > 1 => {
                            pick_step_tab_from_key(modifiers, code, pending.step_count()).map(|target| {
                                let snapshot = snapshot_current_answer(
                                    pending,
                                    &question_answer.read(),
                                    question_selected.get(),
                                    &question_multi_checked.read(),
                                );
                                (target, snapshot)
                            })
                        }
                        _ => None,
                    }
                };
                if let Some((target, snapshot)) = step_tab_jump {
                    let outcome = pending_user_question
                        .write()
                        .take()
                        .map(|pending| pending.jump_to_step(target, snapshot));
                    if let Some(StepNavOutcome::Jumped(pending)) = outcome {
                        apply_step_nav_outcome(
                            StepNavOutcome::Jumped(pending),
                            &mut pending_user_question,
                            &mut question_selected,
                            &mut question_confirm_focus,
                            &mut question_answer,
                            &mut question_multi_checked,
                            &mut question_input_focus,
                            &mut activity_label,
                            &mut question_validation_error,
                        );
                    }
                    return;
                }

                let step_nav_delta = {
                    let pending_ref = pending_user_question.read();
                    match pending_ref.as_ref() {
                        Some(pending)
                            if pending.step_count() > 1
                                && !pending.is_confirm()
                                && !question_input_focus.get().is_custom() =>
                        {
                            question_step_nav_delta(modifiers, code).map(|delta| {
                                let snapshot = snapshot_current_answer(
                                    pending,
                                    &question_answer.read(),
                                    question_selected.get(),
                                    &question_multi_checked.read(),
                                );
                                (delta, snapshot)
                            })
                        }
                        _ => None,
                    }
                };
                if let Some((delta, snapshot)) = step_nav_delta {
                    let outcome = pending_user_question
                        .write()
                        .take()
                        .and_then(|pending| navigate_step_delta(pending, delta, snapshot));
                    if let Some(nav) = outcome {
                        apply_step_nav_outcome(
                            nav,
                            &mut pending_user_question,
                            &mut question_selected,
                            &mut question_confirm_focus,
                            &mut question_answer,
                            &mut question_multi_checked,
                            &mut question_input_focus,
                            &mut activity_label,
                            &mut question_validation_error,
                        );
                    }
                    return;
                }

                let step_back = {
                    let pending_ref = pending_user_question.read();
                    match pending_ref.as_ref() {
                        Some(pending)
                            if pending.can_go_back()
                                && modifiers.is_empty()
                                && code == KeyCode::Backspace
                                && question_input_focus.get().is_choices() =>
                        {
                            let snapshot = snapshot_current_answer(
                                pending,
                                &question_answer.read(),
                                question_selected.get(),
                                &question_multi_checked.read(),
                            );
                            Some(snapshot)
                        }
                        _ => None,
                    }
                };
                if let Some(snapshot) = step_back {
                    let outcome = pending_user_question
                        .write()
                        .take()
                        .and_then(|pending| pending.go_back(snapshot));
                    if let Some(StepNavOutcome::Jumped(pending)) = outcome {
                        apply_step_nav_outcome(
                            StepNavOutcome::Jumped(pending),
                            &mut pending_user_question,
                            &mut question_selected,
                            &mut question_confirm_focus,
                            &mut question_answer,
                            &mut question_multi_checked,
                            &mut question_input_focus,
                            &mut activity_label,
                            &mut question_validation_error,
                        );
                    }
                    return;
                }

                let optional_skip = {
                    let pending_ref = pending_user_question.read();
                    match pending_ref.as_ref() {
                        Some(pending)
                            if !pending.is_required()
                                && !pending.is_confirm()
                                && modifiers.is_empty()
                                && code == KeyCode::Esc =>
                        {
                            Some(())
                        }
                        _ => None,
                    }
                };
                if optional_skip.is_some() {
                    let outcome = pending_user_question
                        .write()
                        .take()
                        .map(|pending| pending.respond(String::new()));
                    if let Some(outcome) = outcome
                        && let Some(summary) = apply_step_submit_outcome(
                            outcome,
                            &mut pending_user_question,
                            &mut question_selected,
                            &mut question_confirm_focus,
                            &mut question_answer,
                            &mut question_multi_checked,
                            &mut question_input_focus,
                            &mut shell_focus,
                            &mut activity_label,
                            &mut question_validation_error,
                        )
                    {
                        push_transcript_message(
                            &mut messages,
                            &mut messages_revision,
                            TranscriptMessage::text(summary, TranscriptStyle::Meta),
                        );
                    }
                    return;
                }

                let approval_choice = {
                    let user_question_active = pending_user_question.read().is_some();
                    if pending_tool_approval.read().is_some() && !user_question_active {
                        if modifiers.is_empty() && code == KeyCode::Esc {
                            Some(ToolApprovalChoice::Reject)
                        } else {
                            pick_tool_approval_index_from_key(modifiers, code)
                                .and_then(choice_at_index)
                                .or_else(|| {
                                    (modifiers.is_empty() && code == KeyCode::Enter)
                                        .then(|| choice_at_index(approval_selected.get()))
                                        .flatten()
                                })
                        }
                    } else {
                        None
                    }
                };
                if let Some(choice) = approval_choice {
                    if let Some(pending) = pending_tool_approval.write().take() {
                        let key = pending.transcript_key();
                        let verb = tool_display_verb(&pending.tool_name);
                        let (style, detail) = match choice {
                            ToolApprovalChoice::Approve => {
                                (TranscriptStyle::StatusSuccess, format!("{verb} · allowed once"))
                            }
                            ToolApprovalChoice::AllowSession => {
                                (TranscriptStyle::StatusSuccess, format!("{verb} · allowed session"))
                            }
                            ToolApprovalChoice::AllowAllTools => {
                                (TranscriptStyle::StatusSuccess, "all tools · session".to_string())
                            }
                            ToolApprovalChoice::Reject => (TranscriptStyle::StatusFailed, format!("{verb} · denied")),
                        };
                        {
                            let mut msgs = messages.write();
                            if let Some(row) = msgs.iter_mut().find(|m| m.startup_key.as_deref() == Some(key.as_str()))
                            {
                                row.content = "Tool approval".to_string();
                                row.status_detail = Some(detail);
                                row.style = style;
                            }
                        }
                        messages_revision.set(messages_revision.get().wrapping_add(1));
                        pending.respond(choice);
                    }
                    shell_focus.set(ShellFocus::Prompt);
                    activity_label.set(match choice {
                        ToolApprovalChoice::Approve => "Running approved tool…".to_string(),
                        ToolApprovalChoice::AllowSession => "Running tool (session allow)…".to_string(),
                        ToolApprovalChoice::AllowAllTools => {
                            "Running tool (all tools allowed this session)…".to_string()
                        }
                        ToolApprovalChoice::Reject => "Tool denied".to_string(),
                    });
                    return;
                }

                let option_nav = {
                    let pending_ref = pending_user_question.read();
                    match (pending_ref.as_ref(), question_option_nav_delta(modifiers, code)) {
                        (Some(pending), Some(delta)) if pending.options().is_some() && !pending.is_confirm() => {
                            let current =
                                current_choice_index(pending, question_selected.get(), question_input_focus.get());
                            advance_question_selection(pending, current, delta)
                        }
                        _ => None,
                    }
                };
                if let Some((next_index, focus)) = option_nav {
                    question_selected.set(next_index);
                    question_input_focus.set(focus);
                    question_validation_error.set(None);
                    return;
                }

                let activate_custom_input = {
                    let pending_ref = pending_user_question.read();
                    match pending_ref.as_ref() {
                        Some(pending)
                            if pending.allow_custom()
                                && question_input_focus.get().is_choices()
                                && is_custom_choice_index(pending, question_selected.get())
                                && modifiers.is_empty()
                                && code == KeyCode::Enter =>
                        {
                            Some(())
                        }
                        _ => None,
                    }
                };
                if activate_custom_input.is_some() {
                    if let Some(pending) = pending_user_question.read().as_ref()
                        && let Some(options) = pending.options()
                    {
                        question_selected.set(options.len());
                    }
                    question_input_focus.set(QuestionInputFocus::Custom);
                    question_validation_error.set(None);
                    return;
                }

                let multi_select_answer = {
                    let pending_ref = pending_user_question.read();
                    match pending_ref.as_ref() {
                        Some(pending)
                            if pending.is_multi_select()
                                && question_input_focus.get().is_choices()
                                && !is_custom_choice_index(pending, question_selected.get())
                                && modifiers.is_empty()
                                && code == KeyCode::Enter =>
                        {
                            let text = question_answer.read().clone();
                            try_resolve_submittable_answer(
                                pending,
                                &text,
                                question_selected.get(),
                                &question_multi_checked.read(),
                            )
                            .ok()
                        }
                        _ => None,
                    }
                };
                if let Some(answer) = multi_select_answer {
                    let outcome = pending_user_question
                        .write()
                        .take()
                        .map(|pending| pending.respond(answer));
                    if let Some(outcome) = outcome
                        && let Some(summary) = apply_step_submit_outcome(
                            outcome,
                            &mut pending_user_question,
                            &mut question_selected,
                            &mut question_confirm_focus,
                            &mut question_answer,
                            &mut question_multi_checked,
                            &mut question_input_focus,
                            &mut shell_focus,
                            &mut activity_label,
                            &mut question_validation_error,
                        )
                    {
                        push_transcript_message(
                            &mut messages,
                            &mut messages_revision,
                            TranscriptMessage::text(summary, TranscriptStyle::Meta),
                        );
                    }
                    return;
                }
                if let Some(pending) = pending_user_question.read().as_ref()
                    && pending.is_multi_select()
                    && question_input_focus.get().is_choices()
                    && !is_custom_choice_index(pending, question_selected.get())
                    && modifiers.is_empty()
                    && code == KeyCode::Enter
                    && let Err(err) = try_resolve_submittable_answer(
                        pending,
                        &question_answer.read(),
                        question_selected.get(),
                        &question_multi_checked.read(),
                    )
                {
                    question_validation_error.set(Some(err));
                    return;
                }

                let picked_option = {
                    let pending_ref = pending_user_question.read();
                    match pending_ref.as_ref() {
                        Some(pending)
                            if pending.is_single_select()
                                && question_input_focus.get().is_choices()
                                && !is_custom_choice_index(pending, question_selected.get())
                                && modifiers.is_empty()
                                && code == KeyCode::Enter =>
                        {
                            let options = pending.options().unwrap_or(&[]);
                            select_value_at(options, question_selected.get())
                        }
                        _ => None,
                    }
                };
                if let Some(value) = picked_option {
                    let outcome = pending_user_question
                        .write()
                        .take()
                        .map(|pending| pending.respond_option(value));
                    if let Some(outcome) = outcome
                        && let Some(summary) = apply_step_submit_outcome(
                            outcome,
                            &mut pending_user_question,
                            &mut question_selected,
                            &mut question_confirm_focus,
                            &mut question_answer,
                            &mut question_multi_checked,
                            &mut question_input_focus,
                            &mut shell_focus,
                            &mut activity_label,
                            &mut question_validation_error,
                        )
                    {
                        push_transcript_message(
                            &mut messages,
                            &mut messages_revision,
                            TranscriptMessage::text(summary, TranscriptStyle::Meta),
                        );
                    }
                    return;
                }

                if !shell_global_shortcut(modifiers, code) {
                    return;
                }
            }

            let prefix_config = PromptPrefixConfig::default();
            let (mirror_draft, mirror_cursor) = prompt_editor_mirror.read().clone();
            let live_body = live_draft.read().clone();
            let stored_body = draft.read().clone();
            let use_mirror = mirror_draft.len() >= live_body.len() && mirror_draft.len() >= stored_body.len();
            let draft_body = if use_mirror {
                mirror_draft
            } else if live_body.len() >= stored_body.len() {
                live_body
            } else {
                stored_body
            };
            let editor_cursor = if use_mirror {
                mirror_cursor.min(draft_body.len())
            } else {
                live_cursor.get().min(draft_body.len())
            };
            let picker_open = input_prefix_kind.get() == InputPrefixKind::Default
                && !status_dialog_open
                && !file_picker_suppressed.get()
                && file_picker_open(&draft_body, editor_cursor);
            if picker_open
                && modifiers.is_empty()
                && matches!(
                    code,
                    KeyCode::Tab | KeyCode::Enter | KeyCode::Right | KeyCode::Up | KeyCode::Down | KeyCode::Esc
                )
            {
                return;
            }
            let draft_text = compose_palette_draft(input_prefix_kind.get(), &draft_body);
            let palette_snapshot = build_snapshot(&draft_text, &slash_commands.read(), screen_height);
            if !status_dialog_open
                && let Some(action) = resolve_snapshot_key_action(
                    &draft_text,
                    &palette_snapshot,
                    slash_palette_index.get(),
                    code,
                    modifiers,
                )
            {
                match action {
                    SlashPaletteKeyAction::CompleteDraft {
                        text: completed,
                        suppress_enter_newline: suppress_enter,
                    } => {
                        let (kind, body) = absorb_inline_triggers(input_prefix_kind.get(), &completed, &prefix_config);
                        input_prefix_kind.set(kind);
                        draft.set(body.clone());
                        live_draft.set(body.clone());
                        live_cursor.set(body.len());
                        suppress_enter_newline.set(suppress_enter);
                        force_palette_sync.set(true);
                        if !palette_visible(&compose_palette_draft(kind, &body)) {
                            slash_palette_active.set(false);
                        }
                        slash_palette_query.write().clear();
                        slash_palette_index.set(0);
                    }
                    SlashPaletteKeyAction::MoveSelection(index) => {
                        slash_palette_index.set(index);
                    }
                    SlashPaletteKeyAction::Dismiss => {
                        draft.set(String::new());
                        live_draft.set(String::new());
                        live_cursor.set(0);
                        input_prefix_kind.set(InputPrefixKind::Default);
                        slash_palette_active.set(false);
                        slash_palette_index.set(0);
                        suppress_enter_newline.set(true);
                    }
                    SlashPaletteKeyAction::SubmitCommand { slash_input } => {
                        input_prefix_kind.set(InputPrefixKind::Default);
                        draft.set(String::new());
                        live_draft.set(String::new());
                        slash_palette_query.write().clear();
                        slash_palette_index.set(0);
                        suppress_enter_newline.set(true);
                        force_palette_sync.set(true);

                        let body = slash_input.trim().trim_start_matches('/').trim().to_string();

                        let extension_registry = extension_host_for_keys.registry();
                        let ext_registry = extension_registry.read();
                        let templates = prompt_templates.read().clone();
                        let loaded_skills = skills.read().clone();
                        let outcome = handle_slash_submit(SlashContext {
                            input: &slash_input,
                            extensions: Some(&ext_registry),
                            prompt_templates: Some(&templates),
                            skills: Some(&loaded_skills),
                            agent_session: agent_session.clone(),
                            extension_host: Some(&extension_host_for_keys),
                            paths: Some(&paths),
                            cwd: Some(&cwd_for_keys),
                        });

                        if slash_echoes_prompt_in_transcript(&outcome) {
                            let mut submitted = TranscriptMessage::text(
                                body.clone(),
                                TranscriptStyle::for_slash_turn_echo(&slash_input),
                            );
                            if submitted.style.is_user_input_card() {
                                submitted.submitted_at = Some(chrono::Utc::now());
                            }
                            push_transcript_message(&mut messages, &mut messages_revision, submitted);
                        }

                        match outcome {
                            SlashOutcome::OpenModelSelector { filter } => {
                                let settings = Settings::load(&paths).ok();
                                open_model_selector(OpenModelSelectorArgs {
                                    pending: &mut pending_model_selector,
                                    provider_index: &mut model_provider_index,
                                    model_index: &mut model_selected_index,
                                    filter: &mut model_filter,
                                    input_focus: &mut model_input_focus,
                                    draft: &mut draft,
                                    live_draft: &mut live_draft,
                                    shell_focus: &mut shell_focus,
                                    initial_filter: filter,
                                    paths: &paths,
                                    provider_id: settings.as_ref().and_then(|s| s.session.provider_id.as_deref()),
                                    model_id: settings.as_ref().and_then(|s| s.session.model_id.as_deref()),
                                    session_scoped: Some(session_scoped_items.read().as_slice()),
                                });
                            }
                            SlashOutcome::OpenScopedModels => {
                                open_scoped_models(OpenScopedModelsArgs {
                                    pending: &mut pending_scoped_models,
                                    selected_index: &mut scoped_selected_index,
                                    filter: &mut scoped_filter,
                                    draft: &mut draft,
                                    live_draft: &mut live_draft,
                                    shell_focus: &mut shell_focus,
                                    paths: &paths,
                                    session_scoped: &session_scoped_items.read(),
                                });
                            }
                            SlashOutcome::OpenSystemPromptDialog { text } => {
                                open_system_prompt_dialog(OpenSystemPromptDialogArgs {
                                    pending: &mut pending_system_prompt,
                                    shell_focus: &mut shell_focus,
                                    text,
                                });
                            }
                            SlashOutcome::PlayConfetti { mode } => {
                                open_confetti(OpenConfettiArgs {
                                    pending: &mut pending_confetti,
                                    state: &mut confetti_runtime,
                                    draft: &mut draft,
                                    live_draft: &mut live_draft,
                                    shell_focus: &mut shell_focus,
                                    mode,
                                });
                            }
                            SlashOutcome::OverlayDeferred(overlay) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(overlay_deferred_message(&overlay), TranscriptStyle::Meta),
                                );
                            }
                            SlashOutcome::Status(message) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(message, TranscriptStyle::Meta),
                                );
                            }
                            SlashOutcome::Assistant(message) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::assistant_slash_markdown(message),
                                );
                            }
                            SlashOutcome::Unimplemented(message) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(message, TranscriptStyle::Meta),
                                );
                            }
                            _ => {}
                        }
                    }
                }
                return;
            }

            let mention_index_ref = mention_index.read();
            let picker_index = mention_index_ref.as_ref().map(|arc| arc.as_ref());
            let file_picker_snapshot = build_file_picker_snapshot(
                &draft_body,
                editor_cursor,
                screen_height,
                file_picker_show_hidden.get(),
                picker_index,
            );
            if picker_open {
                file_picker_active.set(true);
            }
            if !status_dialog_open
                && !palette_snapshot.visible
                && input_prefix_kind.get() == InputPrefixKind::Default
                && file_picker_snapshot.visible
                && modifiers.contains(KeyModifiers::CONTROL)
                && matches!(code, KeyCode::Char('.'))
                && let Some(action) = resolve_file_picker_key_action(
                    &draft_body,
                    editor_cursor,
                    &file_picker_snapshot,
                    file_picker_index.get(),
                    code,
                    modifiers,
                )
                && action == FilePickerKeyAction::ToggleHiddenFiles
            {
                let next = !file_picker_show_hidden.get();
                file_picker_show_hidden.set(next);
                if let Ok(paths) = Paths::resolve()
                    && let Ok(mut settings) = Settings::load_home(&paths)
                {
                    settings.ui.file_picker.show_hidden_files = next;
                    let _ = Settings::save(&paths, &settings);
                }
                let message = if next {
                    "File picker: showing hidden files."
                } else {
                    "File picker: hiding hidden files."
                };
                push_transcript_message(
                    &mut messages,
                    &mut messages_revision,
                    TranscriptMessage::text(message, TranscriptStyle::Meta),
                );
                return;
            }

            if !status_dialog_open
                && shell_focus.get() == ShellFocus::Transcript
                && let Some(ch) = prompt_focus_char(code, modifiers)
            {
                shell_focus.set(ShellFocus::Prompt);
                let body = live_draft.read().clone();
                if let Some(next_kind) = try_consume_trigger(input_prefix_kind.get(), &body, ch, prefix_config.enabled)
                {
                    input_prefix_kind.set(next_kind);
                } else {
                    let mut text = body;
                    text.push(ch);
                    let (kind, normalized) = absorb_inline_triggers(input_prefix_kind.get(), &text, &prefix_config);
                    input_prefix_kind.set(kind);
                    draft.set(normalized.clone());
                    live_draft.set(normalized);
                }
                suppress_enter_newline.set(false);
                return;
            }

            let palette_tab_reserved = palette_snapshot.visible
                || slash_palette_active.get()
                || picker_open
                || file_picker_active.get()
                || file_picker_snapshot.visible;

            match (modifiers, code) {
                (m, KeyCode::Char('l')) | (m, KeyCode::Char('L'))
                    if m.contains(KeyModifiers::CONTROL) && pending_user_question.read().is_none() =>
                {
                    if pending_confetti.read().is_none() {
                        if pending_system_prompt.read().is_some() {
                            close_system_prompt_dialog(
                                &mut pending_system_prompt,
                                &mut draft,
                                &mut live_draft,
                                &mut shell_focus,
                                &mut force_editor_clear,
                            );
                        } else if pending_scoped_models.read().is_some() {
                            cancel_scoped_models(
                                &mut pending_scoped_models,
                                &mut session_scoped_items.write(),
                                &mut draft,
                                &mut live_draft,
                                &mut shell_focus,
                            );
                        } else if pending_model_selector.read().is_some() {
                            close_model_selector(
                                &mut pending_model_selector,
                                &mut draft,
                                &mut live_draft,
                                &mut shell_focus,
                            );
                        } else {
                            let settings = Settings::load(&paths).ok();
                            open_model_selector(OpenModelSelectorArgs {
                                pending: &mut pending_model_selector,
                                provider_index: &mut model_provider_index,
                                model_index: &mut model_selected_index,
                                filter: &mut model_filter,
                                input_focus: &mut model_input_focus,
                                draft: &mut draft,
                                live_draft: &mut live_draft,
                                shell_focus: &mut shell_focus,
                                initial_filter: String::new(),
                                paths: &paths,
                                provider_id: settings.as_ref().and_then(|s| s.session.provider_id.as_deref()),
                                model_id: settings.as_ref().and_then(|s| s.session.model_id.as_deref()),
                                session_scoped: Some(session_scoped_items.read().as_slice()),
                            });
                        }
                    }
                }
                // Ctrl+Y — copy full prompt draft to the system clipboard.
                (m, KeyCode::Char('y')) | (m, KeyCode::Char('Y'))
                    if m.contains(KeyModifiers::CONTROL)
                        && !m.contains(KeyModifiers::SHIFT)
                        && !m.contains(KeyModifiers::ALT)
                        && !status_dialog_open
                        && pending_user_question.read().is_none() =>
                {
                    let expire_tx = ephemeral_expire.read().tx.clone();
                    let banner = if draft_body.is_empty() {
                        prompt_copy_banner(0)
                    } else {
                        match copy_to_clipboard(&draft_body) {
                            Ok(()) => prompt_copy_banner(draft_body.chars().count()),
                            Err(err) => {
                                log::warn!("copy prompt failed: {err}");
                                prompt_copy_failed_banner()
                            }
                        }
                    };
                    show_ephemeral_banner(&mut ephemeral_banner, &mut ephemeral_banner_generation, &expire_tx, banner);
                }
                // Ctrl+Shift+T — roll theme Auto → Light → Dark (persist + reinstall palette).
                (m, KeyCode::Char('t')) | (m, KeyCode::Char('T'))
                    if m.contains(KeyModifiers::CONTROL)
                        && m.contains(KeyModifiers::SHIFT)
                        && !status_dialog_open
                        && pending_user_question.read().is_none() =>
                {
                    if let Some(next) = cycle_and_persist_theme_mode(&paths) {
                        let expire_tx = ephemeral_expire.read().tx.clone();
                        show_ephemeral_banner(
                            &mut ephemeral_banner,
                            &mut ephemeral_banner_generation,
                            &expire_tx,
                            theme_mode_banner(next.label()),
                        );
                    }
                }
                // Ctrl+P / Shift+Ctrl+P — cycle scoped models (pi parity).
                (m, KeyCode::Char('p')) | (m, KeyCode::Char('P'))
                    if m.contains(KeyModifiers::CONTROL)
                        && !status_dialog_open
                        && pending_user_question.read().is_none() =>
                {
                    let reverse = m.contains(KeyModifiers::SHIFT);
                    let agent = agent_session.clone();
                    let (provider, model) = agent
                        .as_ref()
                        .map(|s| (Some(s.model_provider().to_string()), Some(s.model_id().to_string())))
                        .unwrap_or((None, None));
                    let mut stats = chrome_stats.read().clone();
                    match cycle_scoped_model_selection(
                        &paths,
                        &session_scoped_items.read(),
                        provider.as_deref(),
                        model.as_deref(),
                        reverse,
                        &mut stats,
                    ) {
                        Ok((label, value)) => {
                            publish_chrome_stats(&mut chrome_stats, &mut chrome_ui_revision, stats);
                            chrome_refresh_pending.set(true);
                            push_transcript_message(
                                &mut messages,
                                &mut messages_revision,
                                TranscriptMessage::text(format!("Model set to {label}"), TranscriptStyle::Meta),
                            );
                            if let Some(session) = agent {
                                spawn_runtime_model_switch(session, value);
                            }
                        }
                        Err(err) => {
                            push_transcript_message(
                                &mut messages,
                                &mut messages_revision,
                                TranscriptMessage::text(format!("{err}"), TranscriptStyle::Meta),
                            );
                        }
                    }
                }
                (m, KeyCode::Esc) if m.is_empty() && shell_focus.get() == ShellFocus::Transcript => {
                    shell_focus.set(ShellFocus::Prompt);
                }
                // Tab: toggle focus between prompt textarea and transcript.
                (m, KeyCode::Tab) if m.is_empty() && !status_dialog_open && !palette_tab_reserved => {
                    match shell_focus.get() {
                        ShellFocus::Prompt => shell_focus.set(ShellFocus::Transcript),
                        ShellFocus::Transcript => shell_focus.set(ShellFocus::Prompt),
                        ShellFocus::StatusDialog => {}
                    }
                }
                // Shift+Tab: cycle agent mode (BackTab is the usual Shift+Tab code).
                (m, KeyCode::BackTab) | (m, KeyCode::Tab)
                    if !status_dialog_open
                        && !palette_tab_reserved
                        && (matches!(code, KeyCode::BackTab) || m.contains(KeyModifiers::SHIFT)) =>
                {
                    if busy.get() {
                        // Block mode changes during stream/tool work; toast clears async (TTL).
                        let expire_tx = ephemeral_expire.read().tx.clone();
                        show_ephemeral_banner(
                            &mut ephemeral_banner,
                            &mut ephemeral_banner_generation,
                            &expire_tx,
                            agent_mode_busy_banner(),
                        );
                    } else {
                        let next = agent_mode.get().next();
                        agent_mode.set(next);
                        persist_session_prefs(&paths, next, thinking_level.get());
                        let expire_tx = ephemeral_expire.read().tx.clone();
                        show_ephemeral_banner(
                            &mut ephemeral_banner,
                            &mut ephemeral_banner_generation,
                            &expire_tx,
                            agent_mode_banner(next),
                        );
                        if let Some(session) = agent_session.as_ref() {
                            let session = Arc::clone(session);
                            let mode = next;
                            tokio::spawn(async move {
                                if let Err(err) = session.set_agent_mode(mode).await {
                                    log::warn!("failed to set agent mode: {err}");
                                }
                            });
                        }
                    }
                }
                // Ctrl+` / Ctrl+~: cycle thinking level.
                (m, KeyCode::Char('`')) | (m, KeyCode::Char('~')) if m.contains(KeyModifiers::CONTROL) => {
                    let next = thinking_level.get().next();
                    thinking_level.set(next);
                    persist_session_prefs(&paths, agent_mode.get(), next);
                    if let Some(session) = agent_session.as_ref() {
                        let session = Arc::clone(session);
                        let level = next;
                        tokio::spawn(async move {
                            if let Err(err) = session.set_thinking_level(level).await {
                                log::warn!("failed to set thinking level: {err}");
                            }
                        });
                    }
                }
                // Ctrl+O: expand/collapse the most recent finished thinking / tool / response block.
                // Click a process header to toggle that specific older result (iocraft Button hit-test).
                (m, KeyCode::Char(c)) if m.contains(KeyModifiers::CONTROL) && matches!(c, 'o' | 'O') => {
                    let mut msgs = messages.write();
                    if toggle_latest_collapsible_detail(&mut msgs) {
                        drop(msgs);
                        messages_revision.set(messages_revision.get().wrapping_add(1));
                    }
                }
                (m, KeyCode::Char('d')) if m.contains(KeyModifiers::CONTROL) => {
                    let expire_tx = ephemeral_expire.read().tx.clone();
                    let _ = request_quit(
                        PendingQuitAction {
                            pending_quit_confirm: &mut pending_quit_confirm,
                            should_exit: &mut should_exit,
                            busy: &busy,
                            turn_cancel_requested: &mut turn_cancel_requested,
                            prompt_queue: &mut prompt_queue,
                            pending_tool_approval: &mut pending_tool_approval,
                            pending_user_question: &mut pending_user_question,
                            agent_session: &agent_session,
                        },
                        &mut ephemeral_banner,
                        &mut ephemeral_banner_generation,
                        &expire_tx,
                        false,
                    );
                }
                (m, KeyCode::Char('c')) if m.contains(KeyModifiers::CONTROL) && busy.get() => {
                    turn_cancel_requested.set(true);
                    activity_label.set("Cancelling…".to_string());
                    prompt_queue.write().clear();
                    if let Some(pending) = pending_tool_approval.write().take() {
                        pending.respond(ToolApprovalChoice::Reject);
                    }
                    if let Some(question) = pending_user_question.write().take() {
                        question.cancel();
                    }
                    shell_focus.set(ShellFocus::Prompt);
                    question_answer.set(String::new());
                    question_input_focus.set(QuestionInputFocus::Choices);
                    if let Some(token) = user_shell_abort.read().clone() {
                        token.cancel();
                    }
                    if let Some(session) = agent_session.as_ref() {
                        TurnDispatcher::spawn_abort(Arc::clone(session));
                    } else if user_shell_abort.read().is_none() {
                        let canceled_elapsed = busy_started_at
                            .read()
                            .as_ref()
                            .map(|started| format_elapsed_secs(*started))
                            .unwrap_or(0.0);
                        session_elapsed_secs
                            .set(accumulate_session_elapsed(session_elapsed_secs.get(), canceled_elapsed));
                        busy.set(false);
                        busy_started_at.set(None);
                        activity_started_at.set(None);
                        turn_token_tracker.set(None);
                        turn_cancel_requested.set(false);
                        idle_status_notice.set(Some(IdleStatusNotice {
                            text: format_turn_canceled_notice(canceled_elapsed),
                            since: Instant::now(),
                        }));
                    }
                }
                (m, KeyCode::Char('c'))
                    if m.contains(KeyModifiers::CONTROL) && !busy.get() && pending_tool_approval.read().is_none() =>
                {
                    if matches!(handle_prompt_interrupt_text(&draft_text), PromptInterrupt::Cleared) {
                        draft.set(String::new());
                        live_draft.set(String::new());
                        force_editor_clear.set(true);
                        slash_palette_index.set(0);
                        slash_palette_query.write().clear();
                        suppress_enter_newline.set(true);
                    }
                }
                _ => {}
            }
        }
    });

    if should_exit.get() {
        let chrome = chrome_stats.read().clone();
        let api_duration_secs = accumulate_session_elapsed(
            session_elapsed_secs.get(),
            live_turn_elapsed_secs(busy.get(), &busy_started_at.read()),
        );
        let wall_duration_secs = session_wall_started_at.read().elapsed().as_secs_f64();
        let (lines_added, lines_removed) = elph_core::utils::git::read_worktree_stats(paths.read().project_dir())
            .map(|stats| (stats.lines_added, stats.lines_deleted))
            .unwrap_or((0, 0));
        record_if_active(
            ExitSnapshot {
                session_id: session_id.clone(),
                cost_usd: chrome.cost_usd,
                api_duration_secs,
                wall_duration_secs,
                lines_added,
                lines_removed,
                usage: Default::default(),
            },
            count_submitted_user_prompts(&messages.read()),
            chrome.turn_count,
        );
        system.exit();
    }

    let (accent_r, accent_g, accent_b) = agent_mode.get().label_rgb();
    let scanner_accent = rgb(accent_r, accent_g, accent_b);
    let chrome = chrome_stats.read().clone();
    let mcp_connected = agent_session
        .as_ref()
        .and_then(|session| session.mcp_registry())
        .map(|registry| registry.load_report().servers_ok)
        .unwrap_or(0);
    let paths_snapshot = paths.read().clone();
    let project_name = paths_snapshot
        .project_dir()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();
    let git = git_footer.read().clone();
    let model_label = if chrome.model_label.is_empty() {
        fallback_model_label.clone()
    } else {
        chrome.model_label.clone()
    };
    let supports_images = chrome.supports_images;
    let user_question_open = pending_user_question.read().is_some();
    let model_selector_open = pending_model_selector.read().is_some();
    let scoped_models_open = pending_scoped_models.read().is_some();
    let system_prompt_open = pending_system_prompt.read().is_some();
    let confetti_open = pending_confetti.read().is_some();
    let status_dialog_open = pending_tool_approval.read().is_some()
        || user_question_open
        || model_selector_open
        || scoped_models_open
        || system_prompt_open
        || confetti_open;
    let prompt_focused =
        !status_dialog_open && matches!(shell_focus.get(), ShellFocus::Prompt | ShellFocus::StatusDialog);
    let transcript_focused = !status_dialog_open && shell_focus.get() == ShellFocus::Transcript;
    let question_has_focus = user_question_open;
    let model_selector_has_focus =
        model_selector_open && !user_question_open && !system_prompt_open && !confetti_open && !scoped_models_open;
    let scoped_models_has_focus =
        scoped_models_open && !user_question_open && !system_prompt_open && !confetti_open && !model_selector_open;
    let system_prompt_has_focus = system_prompt_open && !confetti_open;
    let approval_has_focus = pending_tool_approval.read().is_some()
        && !user_question_open
        && !model_selector_open
        && !scoped_models_open
        && !system_prompt_open
        && !confetti_open;
    if let Some(pending) = pending_model_selector.write().as_mut() {
        let next_filter = model_selector_sanitize_filter(&model_filter.read());
        if next_filter != model_filter.read().as_str() {
            model_filter.set(next_filter.clone());
        }
        if pending.filter != next_filter {
            pending.model_index = 0;
            model_selected_index.set(0);
        }
        pending.provider_index = model_provider_index.get();
        pending.model_index = model_selected_index.get();
        pending.filter = next_filter;
        pending.input_focus = model_input_focus.get();
        pending.clamp_indices();
        if pending.provider_index != model_provider_index.get() {
            model_provider_index.set(pending.provider_index);
        }
        if pending.model_index != model_selected_index.get() {
            model_selected_index.set(pending.model_index);
        }
    }
    if let Some(pending) = pending_scoped_models.write().as_mut() {
        let next_filter = scoped_filter.read().clone();
        if pending.filter != next_filter {
            pending.set_filter(next_filter);
        }
        pending.selected_index = scoped_selected_index.get();
        pending.clamp_selection();
        if pending.selected_index != scoped_selected_index.get() {
            scoped_selected_index.set(pending.selected_index);
        }
    }
    let model_selector_view = pending_model_selector
        .read()
        .as_ref()
        .map(ModelSelectorView::from_pending);
    let model_selector_overlay = model_selector_view.map(|view| -> AnyElement<'static> {
        element! {
            ModelSelectorBar(
                screen_width: screen_width,
                screen_height: screen_height,
                view: view,
                provider_index: Some(model_provider_index),
                model_index: Some(model_selected_index),
                filter: Some(model_filter),
                input_focus: model_input_focus.get(),
                has_focus: model_selector_has_focus,
                on_filter_submit: move |_| {
                    model_input_focus.set(ModelSelectorFocus::List);
                    if let Some(pending) = pending_model_selector.write().as_mut() {
                        pending.input_focus = ModelSelectorFocus::List;
                    }
                },
                on_confirm: move |_| {},
                on_cancel: move |_| {
                    close_model_selector(
                        &mut pending_model_selector,
                        &mut draft,
                        &mut live_draft,
                        &mut shell_focus,
                    );
                },
            )
        }
        .into()
    });
    let scoped_models_view = pending_scoped_models
        .read()
        .as_ref()
        .map(ScopedModelsView::from_pending);
    let scoped_models_overlay = scoped_models_view.map(|view| -> AnyElement<'static> {
        element! {
            ScopedModelsBar(
                screen_width: screen_width,
                screen_height: screen_height,
                view: view,
                selected_index: Some(scoped_selected_index),
                filter: Some(scoped_filter),
                has_focus: scoped_models_has_focus,
                on_filter_submit: move |_| {
                    if let Some(pending) = pending_scoped_models.write().as_mut() {
                        sync_scoped_filter(pending, &scoped_filter.read());
                        pending.toggle_selected();
                        apply_scoped_session(pending, &mut session_scoped_items.write());
                        scoped_selected_index.set(pending.selected_index);
                    }
                },
                on_cancel: move |_| {
                    cancel_scoped_models(
                        &mut pending_scoped_models,
                        &mut session_scoped_items.write(),
                        &mut draft,
                        &mut live_draft,
                        &mut shell_focus,
                    );
                },
            )
        }
        .into()
    });
    let editor_overlay = model_selector_overlay.or(scoped_models_overlay);
    let _confetti_frame = confetti_frame.get();
    let confetti_overlay = pending_confetti.read().as_ref().map(|_| -> AnyElement<'static> {
        let plane = if let Some(runtime) = confetti_runtime.write().as_mut() {
            runtime.resize(screen_width, screen_height);
            runtime.system.render_plane()
        } else {
            Vec::new()
        };
        element! {
            ConfettiOverlay(
                screen_width: screen_width,
                screen_height: screen_height,
                plane: plane,
            )
        }
        .into()
    });
    let system_prompt_overlay = pending_system_prompt
        .read()
        .as_ref()
        .map(|pending| -> AnyElement<'static> {
            let (chrome, body_height) = system_prompt_dialog_chrome(screen_width, screen_height);
            element! {
                SystemPromptDialogOverlay(
                    screen_width: screen_width,
                    screen_height: screen_height,
                    text: pending.text.clone(),
                    body_height: body_height,
                    chrome: chrome,
                    scroll_handle: Some(system_prompt_scroll),
                    scroll_tick: system_prompt_scroll_tick.get(),
                    has_focus: system_prompt_has_focus,
                )
            }
            .into()
        });
    let user_question_view = pending_user_question.read().as_ref().map(|pending| {
        UserQuestionView::from_pending(
            pending,
            question_input_focus.get(),
            question_selected.get(),
            &question_multi_checked.read(),
            question_validation_error.read().clone(),
        )
    });
    let status_dialog = build_status_dialog_kind(pending_tool_approval.read().as_ref());
    let draft_for_palette = compose_palette_draft(input_prefix_kind.get(), &live_draft.read());
    let draft_body = live_draft.read().clone();
    let editor_cursor = live_cursor.get();
    let slash_palette_snapshot = build_snapshot(&draft_for_palette, &slash_commands.read(), screen_height);
    slash_palette_active.set(slash_palette_snapshot.visible);
    {
        let old_index = slash_palette_index.get();
        let mut query = slash_palette_query.write();
        let mut index = old_index;
        sync_selection(&mut query, &mut index, &slash_palette_snapshot);
        // iocraft marks state dirty on every `.set()` even when the value is unchanged;
        // calling set during render without this guard causes an infinite re-render loop.
        if index != old_index {
            slash_palette_index.set(index);
        }
    }

    if file_picker_suppressed.get() {
        if let Some(mention) = active_mention_at_cursor(&draft_body, editor_cursor)
            && !mention.query.is_empty()
        {
            file_picker_suppressed.set(false);
        } else if !mention_picker_visible(&draft_body, editor_cursor) {
            file_picker_suppressed.set(false);
        }
    }
    if mention_picker_visible(&draft_body, editor_cursor) {
        mention_index_requested.set(true);
    }
    let picker_eligible = input_prefix_kind.get() == InputPrefixKind::Default
        && !slash_palette_snapshot.visible
        && !file_picker_suppressed.get()
        && file_picker_open(&draft_body, editor_cursor);
    let file_picker_snapshot = if picker_eligible {
        build_file_picker_snapshot(
            &draft_body,
            editor_cursor,
            screen_height,
            file_picker_show_hidden.get(),
            mention_index.read().as_ref().map(|arc| arc.as_ref()),
        )
    } else {
        FilePickerSnapshot::hidden()
    };
    file_picker_active.set(picker_eligible);
    styled_content.set(mention_highlight_ansi(&draft_body, editor_cursor));
    {
        let old_index = file_picker_index.get();
        let mut query = file_picker_query.write();
        let mut index = old_index;
        sync_file_picker_selection(&mut query, &mut index, &file_picker_snapshot);
        if index != old_index {
            file_picker_index.set(index);
        }
    }

    element! {
        View(
            width: screen_width,
            height: screen_height,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Center,
            margin: 0,
            padding: 0,
            position: Position::Relative,
            // Keep chrome (incl. footer) inside the terminal; overflow would push the
            // footer row past the last visible line on short screens / first paint.
            overflow: Overflow::Hidden,
        ) {
            Header(
                screen_width: screen_width,
                session_id: session_id.clone(),
                mcp_connected: mcp_connected,
                skills_count: skills_count.get(),
                cost_usd: chrome.cost_usd,
                tokens_used: chrome.tokens_used,
                context_pct: chrome.context_pct,
                context_limit: chrome.context_limit,
                token_display: footer_token_display.clone(),
            )
            TranscriptPanel(
                screen_width: screen_width,
                messages: Some(messages),
                messages_revision: Some(messages_revision),
                sticky_scroll: props.sticky_scroll,
                has_focus: transcript_focused,
            )
            #(user_question_view.map(|view| -> AnyElement<'static> {
                element! {
                    UserQuestionBar(
                        screen_width: screen_width,
                        screen_height: screen_height,
                        view: view,
                        selected_index: Some(question_selected),
                        multi_checked: Some(question_multi_checked),
                        confirm_focus: Some(question_confirm_focus),
                        answer: Some(question_answer),
                        input_focus: question_input_focus.get(),
                        has_focus: question_has_focus,
                        on_confirm_yes: move |_| {
                            let outcome = pending_user_question
                                .write()
                                .take()
                                .map(|question| question.respond_confirm(true));
                            if let Some(outcome) = outcome
                                && let Some(summary) = apply_step_submit_outcome(
                                    outcome,
                                    &mut pending_user_question,
                                    &mut question_selected,
                                    &mut question_confirm_focus,
                                    &mut question_answer,
                                    &mut question_multi_checked,
                                    &mut question_input_focus,
                                    &mut shell_focus,
                                    &mut activity_label,
                                    &mut question_validation_error,
                                )
                            {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(summary, TranscriptStyle::Meta),
                                );
                            }
                        },
                        on_confirm_no: move |_| {
                            let outcome = pending_user_question
                                .write()
                                .take()
                                .map(|question| question.respond_confirm(false));
                            if let Some(outcome) = outcome
                                && let Some(summary) = apply_step_submit_outcome(
                                    outcome,
                                    &mut pending_user_question,
                                    &mut question_selected,
                                    &mut question_confirm_focus,
                                    &mut question_answer,
                                    &mut question_multi_checked,
                                    &mut question_input_focus,
                                    &mut shell_focus,
                                    &mut activity_label,
                                    &mut question_validation_error,
                                )
                            {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(summary, TranscriptStyle::Meta),
                                );
                            }
                        },
                        on_text_submit: move |_| {
                            let answer = {
                                let pending_ref = pending_user_question.read();
                                let Some(pending) = pending_ref.as_ref() else {
                                    return;
                                };
                                let text = question_answer.read().clone();
                                match try_resolve_submittable_answer(
                                    pending,
                                    &text,
                                    question_selected.get(),
                                    &question_multi_checked.read(),
                                ) {
                                    Ok(answer) => answer,
                                    Err(err) => {
                                        question_validation_error.set(Some(err));
                                        return;
                                    }
                                }
                            };
                            let outcome = pending_user_question
                                .write()
                                .take()
                                .map(|question| question.respond(answer));
                            if let Some(outcome) = outcome
                                && let Some(summary) = apply_step_submit_outcome(
                                    outcome,
                                    &mut pending_user_question,
                                    &mut question_selected,
                                    &mut question_confirm_focus,
                                    &mut question_answer,
                                    &mut question_multi_checked,
                                    &mut question_input_focus,
                                    &mut shell_focus,
                                    &mut activity_label,
                                    &mut question_validation_error,
                                )
                            {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(summary, TranscriptStyle::Meta),
                                );
                            }
                        },
                        on_text_cancel: move |_| {
                            if pending_user_question.read().as_ref().is_some_and(|pending| {
                                pending.needs_custom_input()
                                    && !pending.needs_text_input()
                                    && question_input_focus.get().is_custom()
                            }) {
                                question_input_focus.set(QuestionInputFocus::Choices);
                                question_validation_error.set(None);
                                return;
                            }
                            let required = pending_user_question
                                .read()
                                .as_ref()
                                .is_some_and(|pending| pending.needs_text_input() && pending.is_required());
                            let optional_text = pending_user_question
                                .read()
                                .as_ref()
                                .is_some_and(|pending| pending.needs_text_input() && !pending.is_required());
                            if !required && !optional_text {
                                return;
                            }
                            if required {
                                question_answer.set(String::new());
                                question_validation_error.set(None);
                                return;
                            }
                            let outcome = pending_user_question
                                .write()
                                .take()
                                .map(|question| question.respond(String::new()));
                            if let Some(outcome) = outcome
                                && let Some(summary) = apply_step_submit_outcome(
                                    outcome,
                                    &mut pending_user_question,
                                    &mut question_selected,
                                    &mut question_confirm_focus,
                                    &mut question_answer,
                                    &mut question_multi_checked,
                                    &mut question_input_focus,
                                    &mut shell_focus,
                                    &mut activity_label,
                                    &mut question_validation_error,
                                )
                            {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(summary, TranscriptStyle::Meta),
                                );
                            }
                        },
                    )
                }.into()
            }))
            StatusZone(
                screen_width: screen_width,
                screen_height: screen_height,
                busy: busy.get(),
                activity_label: activity_label.read().clone(),
                accent: scanner_accent,
                activity_started_at: activity_started_at.read().clone(),
                busy_started_at: busy_started_at.read().clone(),
                session_elapsed_secs: session_elapsed_secs.get(),
                idle_notice: idle_status_notice.read().as_ref().map(|notice| notice.text.clone()),
                ephemeral_banner: ephemeral_banner
                    .read()
                    .as_ref()
                    .map(|banner| (banner.text.clone(), banner.color())),
                quit_confirm_pending: pending_quit_confirm.get(),
                select_mode: select_mode.get(),
                dialog: status_dialog,
                approval_selected: Some(approval_selected),
                approval_has_focus: approval_has_focus,
            )
            PromptChrome(
                screen_width: screen_width,
                screen_height: screen_height,
                agent_mode: agent_mode.get(),
                thinking_level: thinking_level.get(),
                has_focus: prompt_focused && !select_mode.get(),
                project_name: project_name.clone(),
                git: git.clone(),
                turn: chrome.turn_count,
                model_label: model_label.clone(),
                supports_images: supports_images,
                colored_status_footer: colored_status_footer,
                chrome_revision: chrome_ui_revision.get(),
                draft: Some(draft),
                live_draft: Some(live_draft),
                input_prefix_kind: Some(input_prefix_kind),
                suppress_enter_newline: Some(suppress_enter_newline),
                slash_palette_active: Some(slash_palette_active),
                file_picker_active: Some(file_picker_active),
                styled_content: Some(styled_content),
                live_cursor: Some(live_cursor),
                prompt_editor_mirror: Some(prompt_editor_mirror),
                force_palette_sync: Some(force_palette_sync),
                force_editor_clear: Some(force_editor_clear),
                slash_palette_snapshot: slash_palette_snapshot,
                slash_palette_selected: Some(slash_palette_index),
                file_picker_snapshot: file_picker_snapshot,
                file_picker_selected: Some(file_picker_index),
                file_picker_show_hidden: file_picker_show_hidden.get(),
                editor_overlay: editor_overlay,
                text_select_mode: select_mode.get(),
                blocked_hint: if system_prompt_open {
                    Some("Viewing system prompt — Esc to close".to_string())
                } else if user_question_open {
                    Some("Answer the question above".to_string())
                } else if model_selector_open {
                    Some("Select a model above".to_string())
                } else if scoped_models_open {
                    Some("Edit scoped models above — Ctrl+S save · Esc cancel".to_string())
                } else {
                    None
                },
                on_escape: move |_| {
                    shell_focus.set(ShellFocus::Transcript);
                },
                on_file_picker_key: {
                    let mention_index = mention_index;
                    let mut draft = draft;
                    let mut live_draft = live_draft;
                    let mut live_cursor = live_cursor;
                    let mut file_picker_index = file_picker_index;
                    let mut file_picker_query = file_picker_query;
                    let mut file_picker_active = file_picker_active;
                    let mut file_picker_suppressed = file_picker_suppressed;
                    let mut file_picker_key_handled = file_picker_key_handled;
                    let mut suppress_enter_newline = suppress_enter_newline;
                    let mut force_palette_sync = force_palette_sync;
                    let mut shell_focus = shell_focus;
                    let show_hidden = file_picker_show_hidden.get();
                    move |input: PaletteKeyInput| {
                        let index_ref = mention_index.read();
                        apply_file_picker_key(
                            input,
                            &mut FilePickerApplyContext {
                                screen_height,
                                show_hidden,
                                mention_index: index_ref.as_ref().map(|arc| arc.as_ref()),
                                draft: &mut draft,
                                live_draft: &mut live_draft,
                                live_cursor: &mut live_cursor,
                                file_picker_index: &mut file_picker_index,
                                file_picker_query: &mut file_picker_query,
                                file_picker_active: &mut file_picker_active,
                                file_picker_suppressed: &mut file_picker_suppressed,
                                file_picker_key_handled: &mut file_picker_key_handled,
                                suppress_enter_newline: &mut suppress_enter_newline,
                                force_palette_sync: &mut force_palette_sync,
                                shell_focus: &mut shell_focus,
                            },
                        );
                    }
                },
                file_picker_key_handled: Some(file_picker_key_handled),
                on_submit: move |text: String| {
                        shell_focus.set(ShellFocus::Prompt);
                        if is_force_quit_command(&text) || is_quit_command(&text) {
                            let expire_tx = ephemeral_expire.read().tx.clone();
                            let _ = request_quit(
                                PendingQuitAction {
                                    pending_quit_confirm: &mut pending_quit_confirm,
                                    should_exit: &mut should_exit,
                                    busy: &busy,
                                    turn_cancel_requested: &mut turn_cancel_requested,
                                    prompt_queue: &mut prompt_queue,
                                    pending_tool_approval: &mut pending_tool_approval,
                                    pending_user_question: &mut pending_user_question,
                                    agent_session: &agent_session,
                                },
                                &mut ephemeral_banner,
                                &mut ephemeral_banner_generation,
                                &expire_tx,
                                is_force_quit_command(&text),
                            );
                            draft.set(String::new());
                            live_draft.set(String::new());
                            suppress_enter_newline.set(true);
                            return;
                        }
                        if text.trim().is_empty() {
                            return;
                        }

                        let (prefix_kind, body) =
                            resolve_submit_draft(input_prefix_kind.get(), &text, &PromptPrefixConfig::default());
                        if body.trim().is_empty() {
                            push_transcript_message(
                                &mut messages,
                                &mut messages_revision,
                                TranscriptMessage::text("Empty command.", TranscriptStyle::Meta),
                            );
                            draft.set(String::new());
                            live_draft.set(String::new());
                            suppress_enter_newline.set(true);
                            return;
                        }

                        if matches!(
                            prefix_kind,
                            InputPrefixKind::ShellWithContext | InputPrefixKind::ShellNoContext
                        ) {
                            let with_context = prefix_kind == InputPrefixKind::ShellWithContext;
                            let mut submitted = TranscriptMessage::text(body.clone(), TranscriptStyle::User);
                            submitted.submitted_at = Some(chrono::Utc::now());
                            push_transcript_message(&mut messages, &mut messages_revision, submitted);

                            let tool_id = next_user_shell_tool_id();
                            {
                                let mut msgs = messages.write();
                                if event_applier.write().apply(
                                    &mut msgs,
                                    AgentUiEvent::ToolStart {
                                        id: tool_id.clone(),
                                        name: "shell_exec".into(),
                                        args_summary: shell_exec_args_summary(&body),
                                    },
                                ) {
                                    messages_revision.set(messages_revision.get().wrapping_add(1));
                                }
                            }
                            let shell_activity = user_shell_activity_label(&body);
                            mark_busy(
                                &mut BusyActivation {
                                busy: &mut busy,
                                busy_started_at: &mut busy_started_at,
                                activity_started_at: &mut activity_started_at,
                                activity_label: &mut activity_label,
                                last_activity_label: &mut last_activity_label,
                            },
                                false,
                                Some(&shell_activity),
                            );
                            let abort_token = CancellationToken::new();
                            user_shell_abort.set(Some(abort_token.clone()));
                            spawn_user_shell(
                                Arc::clone(&execution_env),
                                tool_id,
                                body,
                                with_context,
                                abort_token,
                                user_shell_channel.read().tx.clone(),
                            );
                            draft.set(String::new());
                            live_draft.set(String::new());
                            suppress_enter_newline.set(true);
                            return;
                        }

                        let slash_input = if prefix_kind == InputPrefixKind::Slash {
                            format!("/{body}")
                        } else {
                            body.clone()
                        };
                        let is_slash = prefix_kind == InputPrefixKind::Slash;

                        let extension_registry = extension_host.registry();
                        let ext_registry = extension_registry.read();
                        let templates = prompt_templates.read().clone();
                        let loaded_skills = skills.read().clone();
                        let paths_snapshot = paths.read().clone();
                        let outcome = handle_slash_submit(SlashContext {
                            input: &slash_input,
                            extensions: Some(&ext_registry),
                            prompt_templates: Some(&templates),
                            skills: Some(&loaded_skills),
                            agent_session: agent_session.clone(),
                            extension_host: Some(&extension_host),
                            paths: Some(&paths_snapshot),
                            cwd: Some(&cwd),
                        });

                        if slash_echoes_prompt_in_transcript(&outcome) {
                            let mut submitted = TranscriptMessage::text(
                                body.clone(),
                                TranscriptStyle::for_slash_turn_echo(&slash_input),
                            );
                            if submitted.style.is_user_input_card() {
                                submitted.submitted_at = Some(chrono::Utc::now());
                            }
                            push_transcript_message(&mut messages, &mut messages_revision, submitted);
                        }

                        match outcome {
                            SlashOutcome::Quit => {
                                let expire_tx = ephemeral_expire.read().tx.clone();
                                let _ = request_quit(
                                    PendingQuitAction {
                                        pending_quit_confirm: &mut pending_quit_confirm,
                                        should_exit: &mut should_exit,
                                        busy: &busy,
                                        turn_cancel_requested: &mut turn_cancel_requested,
                                        prompt_queue: &mut prompt_queue,
                                        pending_tool_approval: &mut pending_tool_approval,
                                        pending_user_question: &mut pending_user_question,
                                        agent_session: &agent_session,
                                    },
                                    &mut ephemeral_banner,
                                    &mut ephemeral_banner_generation,
                                    &expire_tx,
                                    false,
                                );
                            }
                            SlashOutcome::Status(message) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(message, TranscriptStyle::Meta),
                                );
                            }
                            SlashOutcome::Assistant(message) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::assistant_slash_markdown(message),
                                );
                            }
                            SlashOutcome::Unimplemented(message) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(message, TranscriptStyle::Meta),
                                );
                            }
                            SlashOutcome::OpenModelSelector { filter } => {
                                let settings = Settings::load(&paths_snapshot).ok();
                                open_model_selector(OpenModelSelectorArgs {
                                    pending: &mut pending_model_selector,
                                    provider_index: &mut model_provider_index,
                                    model_index: &mut model_selected_index,
                                    filter: &mut model_filter,
                                    input_focus: &mut model_input_focus,
                                    draft: &mut draft,
                                    live_draft: &mut live_draft,
                                    shell_focus: &mut shell_focus,
                                    initial_filter: filter,
                                    paths: &paths_snapshot,
                                    provider_id: settings.as_ref().and_then(|s| s.session.provider_id.as_deref()),
                                    model_id: settings.as_ref().and_then(|s| s.session.model_id.as_deref()),
                                    session_scoped: Some(session_scoped_items.read().as_slice()),
                                });
                                draft.set(String::new());
                                live_draft.set(String::new());
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenScopedModels => {
                                open_scoped_models(OpenScopedModelsArgs {
                                    pending: &mut pending_scoped_models,
                                    selected_index: &mut scoped_selected_index,
                                    filter: &mut scoped_filter,
                                    draft: &mut draft,
                                    live_draft: &mut live_draft,
                                    shell_focus: &mut shell_focus,
                                    paths: &paths_snapshot,
                                    session_scoped: &session_scoped_items.read(),
                                });
                                draft.set(String::new());
                                live_draft.set(String::new());
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OpenSystemPromptDialog { text } => {
                                open_system_prompt_dialog(OpenSystemPromptDialogArgs {
                                    pending: &mut pending_system_prompt,
                                    shell_focus: &mut shell_focus,
                                    text,
                                });
                                draft.set(String::new());
                                live_draft.set(String::new());
                                force_editor_clear.set(true);
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::PlayConfetti { mode } => {
                                open_confetti(OpenConfettiArgs {
                                    pending: &mut pending_confetti,
                                    state: &mut confetti_runtime,
                                    draft: &mut draft,
                                    live_draft: &mut live_draft,
                                    shell_focus: &mut shell_focus,
                                    mode,
                                });
                                draft.set(String::new());
                                live_draft.set(String::new());
                                suppress_enter_newline.set(true);
                                return;
                            }
                            SlashOutcome::OverlayDeferred(overlay) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(overlay_deferred_message(&overlay), TranscriptStyle::Meta),
                                );
                            }
                            SlashOutcome::SpawnAgentTurn if is_slash => {
                                if agent_session.is_some() {
                                    chrome_refresh_pending.set(true);
                                    idle_status_notice.set(None);
                                    turn_cancel_requested.set(false);
                                    mark_busy(
                                        &mut BusyActivation {
                                busy: &mut busy,
                                busy_started_at: &mut busy_started_at,
                                activity_started_at: &mut activity_started_at,
                                activity_label: &mut activity_label,
                                last_activity_label: &mut last_activity_label,
                            },
                                        false,
                                        None,
                                    );
                                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                                }
                            }
                            SlashOutcome::SpawnAgentTurn => {
                                if busy.get() {
                                    prompt_queue.write().push(body.clone());
                                } else if let Some(session) = agent_session.as_ref() {
                                    chrome_refresh_pending.set(true);
                                    idle_status_notice.set(None);
                                    turn_cancel_requested.set(false);
                                    mark_busy(
                                        &mut BusyActivation {
                                busy: &mut busy,
                                busy_started_at: &mut busy_started_at,
                                activity_started_at: &mut activity_started_at,
                                activity_label: &mut activity_label,
                                last_activity_label: &mut last_activity_label,
                            },
                                        false,
                                        None,
                                    );
                                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                                    TurnDispatcher::spawn_turn(Arc::clone(session), body.clone(), false);
                                } else {
                                    push_transcript_message(
                                        &mut messages,
                                        &mut messages_revision,
                                        TranscriptMessage::text(
                                            "Agent session unavailable — check logs or run `elph doctor`.",
                                            TranscriptStyle::Meta,
                                        ),
                                    );
                                }
                            }
                        }

                    draft.set(String::new());
                    live_draft.set(String::new());
                    suppress_enter_newline.set(true);
                },
            )
            #(confetti_overlay)
            #(system_prompt_overlay)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::parse_skill_slash;
    use elph_agent::Skill;

    fn slash_turn_sets_busy(input: &str, templates: &[PromptTemplate], skills: &[Skill]) -> bool {
        let trimmed = input.trim();
        if trimmed == "/compact" || trimmed == "/c" || trimmed.starts_with("/compact ") || trimmed.starts_with("/c ") {
            return true;
        }
        let body = trimmed.trim_start_matches('/').trim();
        if let Some((name, _)) = parse_skill_slash(body)
            && skills.iter().any(|skill| skill.name == name)
        {
            return true;
        }
        let name = body
            .split_once(' ')
            .map_or(body, |(command, _)| command)
            .to_ascii_lowercase();
        templates.iter().any(|template| template.name == name)
    }

    fn sample_skill() -> Skill {
        Skill {
            name: "tui-design".into(),
            description: "TUI patterns".into(),
            content: "Use iocraft".into(),
            file_path: "/tmp/tui-design/SKILL.md".into(),
            ..Default::default()
        }
    }

    #[test]
    fn slash_turn_sets_busy_for_skill_slash() {
        let skills = vec![sample_skill()];
        assert!(slash_turn_sets_busy("/skill:tui-design layout bug", &[], &skills,));
    }

    #[test]
    fn slash_turn_sets_busy_ignores_unknown_skill() {
        let skills = vec![sample_skill()];
        assert!(!slash_turn_sets_busy("/skill:missing", &[], &skills));
    }
}

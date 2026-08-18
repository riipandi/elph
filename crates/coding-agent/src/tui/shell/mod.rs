//! Root shell: layout zones, global keyboard handling, and session state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
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

use crate::agent::CODEX_TRANSFER_PROMPT_PREFIX;
use crate::agent::CONTINUE_META_LABEL;
use crate::agent::RETRY_CONTINUE_PROMPT;
use crate::agent::TRANSFER_PROMPT_PREFIX;
use crate::agent::load_resources;
use crate::agent::slash_commands_for_palette_with;
use crate::agent::{AgentUiEvent, CodingAgentSession, ToolApprovalChoice};
use crate::extensions::ExtensionHost;
use crate::platform::exit_message::{ExitSnapshot, record_if_active, session_had_user_activity};
use crate::platform::{Paths, Settings};
use crate::types::{AgentMode, SlashCommand, ThinkingLevel};
use crate::types::{is_force_quit_command, is_quit_command};
use crate::utils::path::AppPaths;
use elph_agent::{BUDGET_LIMIT_PROMPT_PREFIX, CONTINUATION_PROMPT_PREFIX};

use crate::agent::rename_session_title;
use crate::agent::session_info_slash_message;
use crate::tui::activity::{TurnCompleteStats, TurnTokenTracker};
use crate::tui::activity::{
    accumulate_session_elapsed, activity_label_for_event, format_quit_canceled_notice, format_shell_canceled_notice,
    format_turn_canceled_notice, format_turn_complete_notice, format_turn_complete_stats_line,
    user_shell_activity_label,
};
use crate::tui::agent_bridge::{PromptQueue, TranscriptEventApplier, TurnDispatcher};
use crate::tui::chrome::{ChromeStats, Header};
use crate::tui::chrome::{chrome_stats_from_session, format_elapsed_secs, read_git_footer_info, refresh_chrome_stats};
use crate::tui::confetti::{
    ConfettiMode, ConfettiOverlay, ConfettiRuntime, OpenConfettiArgs, PendingConfetti, close_confetti, open_confetti,
};
use crate::tui::file_picker::FilePickerKeyAction;
use crate::tui::file_picker::{
    FilePickerApplyContext, FilePickerSnapshot, active_mention_at_cursor, apply_file_picker_key,
    build_snapshot as build_file_picker_snapshot, file_picker_open, mention_highlight_ansi, mention_picker_visible,
    resolve_key_action as resolve_file_picker_key_action, sync_selection as sync_file_picker_selection,
};
use crate::tui::focus::ShellFocus;
use crate::tui::focus::{is_ctrl_enter_interject, is_text_select_toggle_key, prompt_focus_char, shell_global_shortcut};
use crate::tui::item_selector::{
    ItemSelectorPurpose, OpenItemSelectorArgs, PendingItemSelector, apply_tree_filter_key, close_item_selector,
    item_selector_confirm_on_enter, item_selector_confirm_summary_on_ctrl_enter, item_selector_list_nav_delta,
    open_item_selector, tree_filter_key_action,
};
use crate::tui::item_selector_bar::ItemSelectorBar;
use crate::tui::labels::GitFooterInfo;
use crate::tui::mcp_auth_dialog::{
    OpenMcpAuthDialogArgs, PendingMcpAuthDialog, open_mcp_auth_dialog, start_mcp_oauth_for_server,
};
use crate::tui::model_selector::{ModelSelectorFocus, PendingModelSelector};
use crate::tui::model_selector_bar::{ModelSelectorBar, ModelSelectorView};
use crate::tui::model_selector_shell::{
    OpenModelSelectorArgs, apply_model_scoped_action, apply_model_selection_locally, apply_model_selector_filter_seed,
    clamp_thinking_for_model_value, close_model_selector, focus_model_selector_list, model_selector_confirm_on_enter,
    model_selector_filter_seed, model_selector_list_backspace, model_selector_list_nav_delta,
    model_selector_provider_delta, model_selector_sanitize_filter, model_selector_scope_delta,
    model_selector_scoped_action, open_model_selector, spawn_runtime_model_switch, sync_pending_filter,
};
use crate::tui::notifier;
use crate::tui::prompt::PromptChrome;
use crate::tui::prompt_history::is_open_key as is_prompt_history_open_key;
use crate::tui::prompt_history::{
    PromptHistoryKeyAction, build_snapshot as build_prompt_history_snapshot, can_open_history,
    resolve_key_action as resolve_prompt_history_key_action, seed_history_from_transcript,
};
use crate::tui::provider_connect_dialog::{
    OpenProviderApiKeyDialogArgs, OpenProviderConnectDialogArgs, PendingProviderApiKeyDialog,
    PendingProviderConnectDialog, PendingProviderDisconnectDialog, ProviderConnectFocus, ProviderConnectStep,
    apply_provider_filter_seed, close_provider_api_key_dialog, close_provider_connect_dialog,
    close_provider_disconnect_dialog, focus_provider_list, focus_provider_search, format_provider_name,
    get_provider_options_for_auth_method, open_provider_api_key_dialog, open_provider_connect_dialog,
    open_provider_disconnect_dialog, provider_auth_method_from_index, provider_confirm_on_enter, provider_filter_seed,
    provider_list_backspace, provider_list_nav_delta, provider_supports_oauth,
};
use crate::tui::rename_dialog::{
    OpenRenameDialogArgs, PendingRenameDialog, RenameDialogBar, close_rename_dialog, open_rename_dialog,
};
use crate::tui::scoped_models::PendingScopedModels;
use crate::tui::scoped_models_bar::{ScopedModelsBar, ScopedModelsView};
use crate::tui::scoped_models_shell::{
    OpenScopedModelsArgs, apply_scoped_session, cancel_scoped_models, cycle_scoped_model_selection, open_scoped_models,
    save_scoped_models, scoped_models_list_nav_delta, scoped_models_reorder_delta, sync_scoped_filter,
};
use crate::tui::scroll_text_dialog::{
    DEFAULT_SCROLL_TEXT_WIDTH_PCT, OpenScrollTextDialogArgs, ScrollTextDialogOverlay, TOOLS_DIALOG_WIDTH_PCT,
    open_scroll_text_dialog,
};
use crate::tui::session_prefs::cycle_and_persist_theme_mode;
use crate::tui::shell_submit::{
    UserShellEvent, format_shell_agent_context, next_user_shell_tool_id, shell_exec_args_summary, spawn_user_shell,
};
use crate::tui::slash_handler::{SlashContext, SlashOutcome};
use crate::tui::slash_handler::{
    handle_slash_submit, overlay_deferred_message, slash_echoes_prompt_in_transcript, slash_outcome_is_ui_only,
};
use crate::tui::slash_palette::SlashPaletteKeyAction;
use crate::tui::slash_palette::{build_snapshot, palette_visible, resolve_snapshot_key_action, sync_selection};
use crate::tui::startup::{
    BootstrapPhase, BootstrapUiEvent, McpFooterLineKind, TuiBootstrapConfig, append_startup_warning,
    apply_mcp_server_progress, apply_mcp_startup_summary_line, begin_agent_startup, begin_mcp_startup,
    bootstrap_activity_label, bootstrap_is_active, classify_mcp_footer_line, mark_agent_startup_failed,
    mark_agent_startup_ready, mcp_server_status_label, spawn_bootstrap_worker,
};
use crate::tui::status_dialog::{
    PromptQueueAction, StatusDialogKind, StatusZone, build_feedback_dialog_kind, build_mcp_auth_dialog_kind,
    build_memory_flush_dialog_kind, build_mode_change_dialog_kind, build_plan_confirmation_dialog_kind,
    build_prompt_queue_dialog_kind, build_provider_api_key_dialog_kind, build_provider_connect_dialog_kind,
    build_status_dialog_kind,
};
use crate::tui::subagent_output_dialog::{PendingSubagentOutputDialog, SubagentOutputDialogOverlay};
use crate::tui::system_prompt_dialog::{
    OpenSystemPromptDialogArgs, PendingSystemPromptDialog, close_system_prompt_dialog, open_system_prompt_dialog,
    system_prompt_dialog_chrome,
};
use crate::tui::tool_approval::{
    FEEDBACK_DEFAULT_INDEX, PLAN_CONFIRM_DEFAULT_INDEX, PendingMemoryFlush, PendingModeChange, PendingPlanConfirmation,
    PendingToolApproval, PlanChoice, TOOL_APPROVAL_DEFAULT_INDEX, choice_at_index_for, feedback_url_at_index, open_url,
    pick_feedback_index_from_key, pick_memory_flush_index_from_key, pick_mode_change_index_from_key,
    pick_tool_approval_index_from_key_for, plan_confirmation_transcript_key, to_harness_choice,
    tool_approval_transcript_key,
};
use crate::tui::tool_params::tool_display_verb;
use crate::tui::transcript::{
    AGENT_MODE_NOTICE_TTL, EphemeralBanner, EphemeralBannerGeneration, EphemeralBannerKind,
    FILE_PICKER_HIDDEN_NOTICE_KEY, LogDensity, MODEL_SET_NOTICE_KEY, QUIT_BUSY_NOTICE_KEY, TranscriptMessage,
    TranscriptPanel, TranscriptStyle, agent_mode_banner, agent_mode_busy_banner, api_error_banner,
    apply_transcript_retention, clear_ephemeral_banner, clear_ephemeral_banner_if_generation, clipboard_notice_banner,
    expire_ephemeral_banner, file_picker_hidden_notice_text, model_set_notice_from_value, model_set_notice_text,
    prompt_copy_banner, prompt_copy_failed_banner, publish_ephemeral_banner, quit_busy_banner, select_mode_off_banner,
    select_mode_on_banner, theme_mode_banner, toggle_latest_collapsible_detail,
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
use elph_tui::copy_to_clipboard;

mod ctx;
mod helpers;
mod keys;
mod tick;
mod view;

use ctx::ShellCtx;
use helpers::*;
use keys::handle_shell_key;
use tick::shell_tick_loop;
use view::build_shell_view;

// Re-exported for transcript reconstruction on session resume (startup.rs).
pub(crate) use helpers::worker_inbound_meta_label;

// ── OAuth dialog events ──────────────────────────────────────────────

/// Events sent from the OAuth flow to the provider connect dialog.
#[derive(Debug, Clone)]
enum OAuthDialogEvent {
    DeviceCode {
        url: String,
        code: String,
    },
    /// Prompt the user for text input (e.g. GitHub Copilot enterprise URL).
    PromptText {
        #[allow(dead_code)]
        id: u64,
        message: String,
        #[allow(dead_code)]
        placeholder: Option<String>,
    },
    /// Prompt the user to paste a manual authorization code / redirect URL.
    PromptManualCode {
        #[allow(dead_code)]
        id: u64,
        message: String,
        placeholder: Option<String>,
    },
    /// Prompt the user to select from a list of options (e.g. OpenAI Codex login method).
    PromptSelect {
        #[allow(dead_code)]
        id: u64,
        message: String,
        options: Vec<elph_ai::auth::AuthSelectOption>,
    },
}

/// Global store for OAuth prompt response channels.
/// Keyed by prompt_id, value is the oneshot sender to deliver the response.
static OAUTH_PROMPT_STORE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<u64, tokio::sync::oneshot::Sender<String>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// AuthLoginCallbacks implementation that forwards events via an mpsc channel.
struct OAuthLoginCallbacksImpl {
    tx: tokio::sync::mpsc::UnboundedSender<OAuthDialogEvent>,
}

impl elph_ai::auth::AuthLoginCallbacks for OAuthLoginCallbacksImpl {
    fn prompt(&self, prompt: elph_ai::auth::AuthPrompt) -> elph_ai::auth::BoxFuture<'_, anyhow::Result<String>> {
        let tx = self.tx.clone();
        let prompt_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        // Register the oneshot **before** notifying the UI so a fast Enter (e.g. blank
        // enterprise host → github.com) is never dropped on an empty store.
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        {
            let mut store = OAUTH_PROMPT_STORE.lock().unwrap();
            store.insert(prompt_id, response_tx);
        }

        match &prompt {
            elph_ai::auth::AuthPrompt::Text { message, placeholder } => {
                let _ = tx.send(OAuthDialogEvent::PromptText {
                    id: prompt_id,
                    message: message.clone(),
                    placeholder: placeholder.clone(),
                });
            }
            elph_ai::auth::AuthPrompt::ManualCode { message, placeholder } => {
                let _ = tx.send(OAuthDialogEvent::PromptManualCode {
                    id: prompt_id,
                    message: message.clone(),
                    placeholder: placeholder.clone(),
                });
            }
            elph_ai::auth::AuthPrompt::Secret { message, placeholder } => {
                let _ = tx.send(OAuthDialogEvent::PromptText {
                    id: prompt_id,
                    message: message.clone(),
                    placeholder: placeholder.clone(),
                });
            }
            elph_ai::auth::AuthPrompt::Select { message, options } => {
                let _ = tx.send(OAuthDialogEvent::PromptSelect {
                    id: prompt_id,
                    message: message.clone(),
                    options: options.clone(),
                });
            }
        }

        Box::pin(async move {
            match response_rx.await {
                Ok(response) => Ok(response),
                Err(_) => Err(anyhow::anyhow!("OAuth prompt cancelled")),
            }
        })
    }

    fn notify(&self, event: elph_ai::auth::AuthEvent) {
        match event {
            elph_ai::auth::AuthEvent::DeviceCode {
                user_code,
                verification_uri,
                ..
            } => {
                let _ = self.tx.send(OAuthDialogEvent::DeviceCode {
                    url: verification_uri,
                    code: user_code,
                });
            }
            elph_ai::auth::AuthEvent::AuthUrl { url, .. } => {
                let _ = self.tx.send(OAuthDialogEvent::DeviceCode {
                    url: url.clone(),
                    code: String::new(),
                });
            }
            _ => {}
        }
    }
}

// ── Constants ────────────────────────────────────────────────────────

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
    /// Transcript log density for collapsed tool-call items (see `settings.ui.density`).
    pub density: LogDensity,
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
    pub allow_mode_change_while_busy: bool,
    /// When true (default), show the dimmed per-turn stats card (tokens/model) after each completed turn.
    pub turn_stats_enabled: bool,
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
            density: LogDensity::Compact,
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
            allow_mode_change_while_busy: true,
            turn_stats_enabled: true,
            initial_git_footer: None,
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
    let should_exit = hooks.use_state(|| false);
    // Track whether Shift is held so the transcript can hide the scrollbar and disable
    // mouse capture during native text selection (like a temporary Ctrl+S toggle).
    // Declared early so mouse capture can reference it.
    let shift_held = hooks.use_state(|| false);
    let shift_last_pressed = hooks.use_ref(|| None::<Instant>);
    // When true, mouse capture is off so the terminal can native-select transcript text.
    let select_mode = hooks.use_state(|| false);
    // Apply every frame; iocraft only reconfigures the terminal when the value changes.
    // Shift-held mode also disables mouse capture (temporary Ctrl+S toggle).
    system.set_mouse_capture(!select_mode.get() && !shift_held.get());
    let agent_mode = hooks.use_state(|| props.initial_agent_mode);
    let thinking_level = hooks.use_state(|| props.initial_thinking_level);
    let draft = hooks.use_state(String::new);
    let live_draft = hooks.use_ref(String::new);
    let input_prefix_kind = hooks.use_ref(InputPrefixKind::default);
    let startup_messages = props.startup_messages.clone();
    let messages = hooks.use_state(move || startup_messages);
    let startup_messages_arc = props.startup_messages.clone();
    let messages_arc = hooks.use_ref(move || Arc::new(RwLock::new(startup_messages_arc)));

    let messages_revision = hooks.use_state(|| 0u64);
    let suppress_enter_newline = hooks.use_ref(|| false);
    let slash_palette_active = hooks.use_ref(|| false);
    let file_picker_active = hooks.use_ref(|| false);
    let file_picker_suppressed = hooks.use_ref(|| false);
    let file_picker_key_handled = hooks.use_ref(|| false);
    let prompt_history_open = hooks.use_state(|| false);
    let prompt_history_index = hooks.use_state(|| 0usize);
    let prompt_history = hooks.use_ref(Vec::<String>::new);
    // Far enough in the past so the first deliberate Arrow Up is never treated as a burst.
    let last_arrow_up_at = hooks.use_ref(|| Instant::now() - Duration::from_secs(1));
    let force_palette_sync = hooks.use_ref(|| false);
    let force_editor_clear = hooks.use_ref(|| false);
    let busy = hooks.use_state(|| false);
    // True only while a real agent turn is running (not bootstrap / local shell).
    // Queue + interject use this — not bare `busy` (bootstrap sets busy while harness is idle).
    let agent_turn_active = hooks.use_state(|| false);
    let activity_label = hooks.use_state(|| "Thinking".to_string());
    let session_elapsed_secs = hooks.use_state(|| 0.0f64);
    let session_wall_started_at = hooks.use_ref(Instant::now);
    let show_thinking = props.show_thinking;
    let auto_expand_thinking = props.auto_expand_thinking;
    // Install the transcript log-density preference process-wide (layout/measure + paint
    // read one shared value). Compact by default; tests toggle it directly.
    crate::tui::transcript::set_log_density(props.density);
    let density = props.density;
    let busy_started_at = hooks.use_ref(|| None::<Instant>);
    let activity_started_at = hooks.use_ref(|| None::<Instant>);
    let last_activity_label = hooks.use_ref(String::new);
    let prompt_queue = hooks.use_ref(PromptQueue::default);
    let queue_manager_open = hooks.use_state(|| false);
    let queue_manager_selected = hooks.use_state(|| 0usize);
    let queue_manager_action = hooks.use_state(PromptQueueAction::default);
    // Mouse clicks on queue action chips (drained each frame in the shell body).
    let pending_queue_click = hooks.use_state(|| None::<(usize, PromptQueueAction)>);
    let on_queue_action_click = hooks.use_async_handler(move |(idx, action)| {
        let mut pending_queue_click = pending_queue_click;
        async move {
            pending_queue_click.set(Some((idx, action)));
        }
    });
    // Bumped on queue mutations so MainShell re-renders (prompt_queue is a Ref).
    let queue_ui_revision = hooks.use_state(|| 0u64);
    // Shell already pushed a user card; skip matching `UserPromptCommitted` from the agent loop.
    let pre_echoed_user_prompts = hooks.use_state(|| 0u32);
    let event_applier = hooks.use_ref(|| TranscriptEventApplier::new(props.show_thinking, props.auto_expand_thinking));
    let pending_tool_approval = hooks.use_ref(|| None::<PendingToolApproval>);
    let pending_plan_confirmation = hooks.use_ref(|| None::<PendingPlanConfirmation>);
    let pending_feedback = hooks.use_ref(|| false);
    let pending_memory_flush = hooks.use_ref(|| None::<PendingMemoryFlush>);
    let pending_user_question = hooks.use_ref(|| None::<PendingUserQuestion>);
    let slash_commands = hooks.use_state(|| props.slash_commands.clone());
    let prompt_templates = hooks.use_state(|| props.prompt_templates.clone());
    let skills = hooks.use_state(|| props.skills.clone());
    let slash_palette_index = hooks.use_state(|| 0usize);
    let slash_palette_query = hooks.use_ref(String::new);
    let file_picker_index = hooks.use_state(|| 0usize);
    let file_picker_query = hooks.use_ref(String::new);
    let live_cursor = hooks.use_ref(|| 0usize);
    // Plain-`y` selection yank toast from Textarea — drained into ephemeral banner.
    let clipboard_toast = hooks.use_state(|| None::<elph_tui::ClipboardNotice>);
    let pending_mode_change = hooks.use_ref(|| None::<PendingModeChange>);
    let pending_retry_prompt = hooks.use_ref(|| None::<String>);
    let prompt_editor_mirror = hooks.use_ref(|| (String::new(), 0usize));
    let styled_content = hooks.use_ref(String::new);
    let mention_index = hooks.use_ref(|| None::<Arc<MentionSearchIndex>>);
    let mention_index_requested = hooks.use_ref(|| false);
    let file_picker_show_hidden = hooks.use_state(|| props.file_picker_show_hidden);
    let allow_mode_change_while_busy = hooks.use_state(|| props.allow_mode_change_while_busy);
    let palette_refresh_pending = hooks.use_state(|| false);
    let shell_focus = hooks.use_state(ShellFocus::default);
    let question_selected = hooks.use_state(|| 0usize);
    let question_confirm_focus = hooks.use_state(ConfirmButtonFocus::default);
    let question_answer = hooks.use_state(String::new);
    let question_multi_checked = hooks.use_state(Vec::<bool>::new);
    let question_input_focus = hooks.use_state(QuestionInputFocus::default);
    let question_validation_error = hooks.use_state(|| None::<String>);
    let approval_selected = hooks.use_state(|| 0usize);
    let pending_model_selector = hooks.use_ref(|| None::<crate::tui::model_selector::PendingModelSelector>);
    let model_provider_index = hooks.use_state(|| 0usize);
    let model_selected_index = hooks.use_state(|| 0usize);
    let model_filter = hooks.use_state(String::new);
    let model_input_focus = hooks.use_state(ModelSelectorFocus::default);
    let pending_scoped_models = hooks.use_ref(|| None::<PendingScopedModels>);
    let scoped_selected_index = hooks.use_state(|| 0usize);
    let scoped_filter = hooks.use_state(String::new);
    let session_scoped_items = hooks.use_ref(|| {
        Settings::load(&props.paths)
            .map(|s| s.models.scoped_models)
            .unwrap_or_default()
    });
    let pending_system_prompt = hooks.use_ref(|| None::<PendingSystemPromptDialog>);
    let pending_aside = hooks.use_ref(|| None::<crate::tui::aside_panel::AsidePanelState>);
    let aside_tick = hooks.use_state(|| 0u64);
    let pending_worker_chat = hooks.use_ref(|| None::<crate::tui::worker_chat::WorkerChatState>);
    let worker_chat_selected = hooks.use_state(|| 0usize);
    // Pending inbound worker messages not yet seen (overlay closed). Drives the
    // footer `⬡` badge color: >0 → yellow. Reset when the overlay opens.
    let worker_pending_count = hooks.use_state(|| 0usize);
    let system_prompt_scroll = hooks.use_ref_default::<ScrollViewHandle>();
    let system_prompt_scroll_tick = hooks.use_ref(|| 0u32);
    let pending_rename = hooks.use_ref(|| None::<crate::tui::rename_dialog::PendingRenameDialog>);
    let rename_value = hooks.use_state(String::new);
    let pending_item_selector = hooks.use_ref(|| None::<PendingItemSelector>);
    let item_selector_selected = hooks.use_state(|| 0usize);
    let pending_confetti = hooks.use_ref(|| None::<PendingConfetti>);
    let pending_provider_connect = hooks.use_ref(|| None::<PendingProviderConnectDialog>);
    let pending_provider_disconnect = hooks.use_ref(|| None::<PendingProviderDisconnectDialog>);
    let pending_provider_api_key = hooks.use_ref(|| None::<PendingProviderApiKeyDialog>);
    let pending_mcp_auth = hooks.use_ref(|| None::<PendingMcpAuthDialog>);
    let provider_disconnect_selected = hooks.use_state(|| 0usize);
    let provider_connect_selected = hooks.use_state(|| 0usize);
    let provider_connect_filter = hooks.use_state(String::new);
    let provider_connect_api_key = hooks.use_state(String::new);
    let provider_connect_input_focus = hooks.use_state(ProviderConnectFocus::default);
    let confetti_runtime = hooks.use_ref(|| None::<crate::tui::confetti::ConfettiRuntime>);
    let confetti_frame = hooks.use_state(|| 0u32);

    // Shared output buffers for real-time subagent dialog display.
    // Maps agent_id → (text: Arc<RwLock<String>>, is_running: Arc<AtomicBool>).
    // The shell writes SubagentOutput events into these buffers; the dialog reads them.
    // Using Arc<RwLock> so both the async event loop and the render phase can access.
    let subagent_output_buffers: Arc<RwLock<HashMap<String, SubagentBuf>>> = Arc::new(RwLock::new(HashMap::new()));
    let subagent_output_buffers_state = hooks.use_ref(|| subagent_output_buffers.clone());
    let subagent_output_scroll_tick = hooks.use_state(|| 0u32);
    // Pending dialog state — when Some, the SubagentOutputDialogOverlay is rendered.
    let pending_subagent_output =
        hooks.use_ref(|| None::<crate::tui::subagent_output_dialog::PendingSubagentOutputDialog>);
    // Scroll handle for subagent output dialog.
    let subagent_output_scroll = hooks.use_ref_default::<ScrollViewHandle>();

    let extension_host = props.extension_host.clone();
    let cwd = props.cwd.clone();

    let agent_session_slot = hooks.use_ref(|| props.agent_session.clone());
    let ui_events_slot = hooks.use_ref(|| props.ui_events.clone());
    let bootstrap_phase = hooks.use_ref(|| {
        if props.bootstrap.is_some() {
            BootstrapPhase::Pending
        } else {
            BootstrapPhase::Done
        }
    });
    let bootstrap_config = hooks.use_ref(|| props.bootstrap.clone());
    let bootstrap_worker_started = hooks.use_ref(|| false);
    let bootstrap_rx = hooks.use_ref(|| None::<UnboundedReceiver<BootstrapUiEvent>>);
    let live_session_id = hooks.use_state(|| props.session_id.clone());
    let extension_host_for_palette = extension_host.clone();
    let execution_env = props.execution_env.clone();
    let user_shell_channel = hooks.use_ref(|| {
        let (tx, rx) = unbounded_channel();
        UserShellChannel { tx, rx }
    });
    let user_shell_abort = hooks.use_ref(|| None::<CancellationToken>);
    let paths = hooks.use_state(|| props.paths.clone());
    let skills_count = hooks.use_state(|| 0usize);
    let chrome_refresh_pending = hooks.use_state(|| true);
    let chrome_stats = hooks.use_state(|| ChromeStats {
        context_limit: props.context_limit,
        model_label: props.model_label.clone(),
        supports_images: props.supports_images,
        ..ChromeStats::default()
    });
    let git_footer = hooks.use_state(|| props.initial_git_footer.clone());
    // Start at 1 so the first Footer paint depends on chrome_revision (iocraft child identity).
    let chrome_ui_revision = hooks.use_state(|| 1u64);
    let chrome_tick = hooks.use_ref(|| 0u32);
    // A state update only recomputes the canvas; it cannot repair terminal cells overwritten by
    // startup output when the resulting pixels are unchanged. Request one full terminal rewrite
    // after the first chrome pass and after each bootstrap event.
    let chrome_eager_paint_done = hooks.use_ref(|| false);
    let chrome_full_redraw_pending = hooks.use_ref(|| false);
    let fallback_context_limit = props.context_limit;
    let fallback_model_label = props.model_label.clone();
    let fallback_model_label_for_chrome = fallback_model_label.clone();
    let fallback_supports_images = props.supports_images;
    let footer_token_display = props.footer_token_display.clone();
    let colored_status_footer = props.colored_status_footer;
    let session_id = live_session_id.read().clone();
    let transcript_pending = hooks.use_ref(|| false);
    let last_transcript_publish = hooks.use_ref(|| Instant::now() - Duration::from_millis(TRANSCRIPT_PUBLISH_MS));
    let last_event_burst = hooks.use_ref(|| 0usize);
    let idle_status_notice = hooks.use_ref(|| None::<IdleStatusNotice>);
    let turn_cancel_requested = hooks.use_ref(|| false);
    let pending_quit_confirm = hooks.use_ref(|| false);
    let turn_token_tracker = hooks.use_ref(|| None::<TurnTokenTracker>);
    // Stats of the most recently completed turn (usage/model) for the dimmed transcript card.
    let last_turn_stats = hooks.use_ref(|| None::<TurnCompleteStats>);
    // `ui.turnStats` — show the dimmed per-turn stats card after each completed turn.
    let turn_stats_enabled = props.turn_stats_enabled;
    // Track if an approval dialog (mode change / tool approval) set the activity label.
    // Cleared on RunCompleted to reset status when turn finishes.
    let pending_approval_label = hooks.use_ref(|| false);
    // Path to the plan file being actively implemented (set on Implement, cleared on RunCompleted).
    // Used to transition frontmatter `Status` from `in_progress` to `completed` when the turn finishes.
    let active_plan_file = hooks.use_ref(|| None::<String>);
    // Fixed toast above status row (agent mode, quit-busy) — not in the scrollable transcript.
    // State (not Ref) so set/clear repaints without waiting for agent busy/stream updates.
    let ephemeral_banner = hooks.use_state(|| None::<EphemeralBanner>);
    let ephemeral_banner_generation = hooks.use_ref(EphemeralBannerGeneration::default);
    let ephemeral_expire = hooks.use_ref(|| {
        let (tx, rx) = unbounded_channel();
        EphemeralExpireChannel { tx, rx }
    });
    // Auto-clear schedule for transcript `transient:*` notices (file-picker, model set, …).
    let pending_transcript_notice_expires = hooks.use_ref(HashMap::<&'static str, Instant>::new);

    // Set true by `/new` handler; the tick loop picks this up to reload resources + restart
    // bootstrap with a fresh session (in-process, no exit + re-launch).
    let new_session_requested = hooks.use_ref(|| false);
    // `/resume <id>` — next tick reloads bootstrap with this session id.
    let resume_session_requested = hooks.use_ref(|| None::<String>);

    let cwd_for_mention_index = cwd.clone();
    let cwd_for_loop = cwd.clone();
    let extension_host_for_loop = extension_host.clone();
    let pending_provider_connect_for_tick = pending_provider_connect;
    let pending_provider_disconnect_for_tick = pending_provider_disconnect;
    let pending_mcp_auth_for_tick = pending_mcp_auth;
    let messages_for_tick = messages;
    let messages_revision_for_tick = messages_revision;
    let provider_connect_api_key_for_tick = provider_connect_api_key;
    let provider_connect_input_focus_for_tick = provider_connect_input_focus;
    let shell_focus_for_tick = shell_focus;
    let layout_screen_size_for_loop = layout_screen_size;
    // Clone the Arc into the async future so event processing writes to
    // messages_arc (no State dirty marks) instead of messages State.
    let messages_arc_inner: Arc<RwLock<Vec<TranscriptMessage>>> = messages_arc.read().clone();

    // Bundle every shell state handle once; each extracted block clones the bundle
    // and destructures only the handles it needs.
    let agent_session = agent_session_slot.read().clone();
    let ctx = ShellCtx {
        active_plan_file,
        activity_label,
        activity_started_at,
        agent_mode,
        agent_session,
        agent_session_slot,
        agent_turn_active,
        allow_mode_change_while_busy,
        approval_selected,
        auto_expand_thinking,
        bootstrap_config,
        bootstrap_phase,
        bootstrap_rx,
        bootstrap_worker_started,
        busy,
        busy_started_at,
        chrome_eager_paint_done,
        chrome_full_redraw_pending,
        chrome_refresh_pending,
        chrome_stats,
        chrome_tick,
        chrome_ui_revision,
        clipboard_toast,
        colored_status_footer,
        confetti_frame,
        confetti_runtime,
        cwd,
        cwd_for_loop,
        cwd_for_mention_index,
        draft,
        ephemeral_banner,
        ephemeral_banner_generation,
        ephemeral_expire,
        event_applier,
        execution_env,
        extension_host,
        extension_host_for_loop,
        extension_host_for_palette,
        fallback_context_limit,
        fallback_model_label,
        fallback_model_label_for_chrome,
        fallback_supports_images,
        file_picker_active,
        file_picker_index,
        file_picker_key_handled,
        file_picker_query,
        file_picker_show_hidden,
        file_picker_suppressed,
        footer_token_display,
        force_editor_clear,
        force_palette_sync,
        git_footer,
        idle_status_notice,
        input_prefix_kind,
        last_activity_label,
        last_arrow_up_at,
        last_event_burst,
        last_transcript_publish,
        layout_screen_size_for_loop,
        live_cursor,
        live_draft,
        live_session_id,
        mention_index,
        mention_index_requested,
        messages,
        messages_arc,
        messages_arc_inner,
        messages_for_tick,
        messages_revision,
        messages_revision_for_tick,
        model_filter,
        model_input_focus,
        model_provider_index,
        model_selected_index,
        density,
        new_session_requested,
        resume_session_requested,
        on_queue_action_click,
        palette_refresh_pending,
        paths,
        pending_confetti,
        pending_feedback,
        pending_memory_flush,
        pending_mode_change,
        pending_model_selector,
        pending_retry_prompt,
        pending_plan_confirmation,
        pending_provider_api_key,
        pending_mcp_auth,
        pending_mcp_auth_for_tick,
        pending_provider_connect,
        pending_provider_connect_for_tick,
        pending_provider_disconnect,
        pending_provider_disconnect_for_tick,
        pending_queue_click,
        pending_quit_confirm,
        pending_rename,
        pending_item_selector,
        item_selector_selected,
        pending_scoped_models,
        pending_subagent_output,
        pending_system_prompt,
        pending_aside,
        aside_tick,
        pending_worker_chat,
        worker_chat_selected,
        worker_pending_count,
        pending_tool_approval,
        pending_transcript_notice_expires,
        pending_user_question,
        pre_echoed_user_prompts,
        prompt_editor_mirror,
        prompt_history,
        prompt_history_index,
        prompt_history_open,
        prompt_queue,
        prompt_templates,
        provider_connect_api_key,
        provider_connect_api_key_for_tick,
        provider_connect_filter,
        provider_connect_input_focus,
        provider_connect_input_focus_for_tick,
        provider_connect_selected,
        provider_disconnect_selected,
        question_answer,
        question_confirm_focus,
        question_input_focus,
        question_multi_checked,
        question_selected,
        question_validation_error,
        queue_manager_action,
        queue_manager_open,
        queue_manager_selected,
        queue_ui_revision,
        rename_value,
        scoped_filter,
        scoped_selected_index,
        screen_height,
        screen_width,
        select_mode,
        session_elapsed_secs,
        session_id,
        session_scoped_items,
        session_wall_started_at,
        shell_focus,
        shell_focus_for_tick,
        shift_held,
        shift_last_pressed,
        should_exit,
        show_thinking,
        skills,
        skills_count,
        slash_commands,
        slash_palette_active,
        slash_palette_index,
        slash_palette_query,
        styled_content,
        subagent_output_buffers,
        subagent_output_buffers_state,
        subagent_output_scroll,
        subagent_output_scroll_tick,
        suppress_enter_newline,
        system_prompt_scroll,
        system_prompt_scroll_tick,
        thinking_level,
        transcript_pending,
        turn_cancel_requested,
        turn_token_tracker,
        last_turn_stats,
        turn_stats_enabled,
        pending_approval_label,
        ui_events_slot,
        user_shell_abort,
        user_shell_channel,
        todos: hooks.use_state(Vec::new),
    };

    hooks.use_future(shell_tick_loop(ctx.clone()));

    hooks.use_terminal_events({
        let ctx_for_keys = ctx.clone();
        move |event| handle_shell_key(ctx_for_keys.clone(), event)
    });

    build_shell_view(system, ctx, props.sticky_scroll)
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
        // Legacy `/skill:name` prefix.
        if let Some((name, _)) = parse_skill_slash(body)
            && skills.iter().any(|skill| skill.name == name)
        {
            return true;
        }
        let name = body
            .split_once(' ')
            .map_or(body, |(command, _)| command)
            .to_ascii_lowercase();
        // Match by raw name (skills, templates).
        if skills.iter().any(|skill| skill.name.to_ascii_lowercase() == name) {
            return true;
        }
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
        assert!(slash_turn_sets_busy("/tui-design layout bug", &[], &skills,));
        // Legacy `/skill:` prefix still works for busy detection.
        assert!(slash_turn_sets_busy("/skill:tui-design layout bug", &[], &skills,));
    }

    #[test]
    fn slash_turn_sets_busy_ignores_unknown_skill() {
        let skills = vec![sample_skill()];
        assert!(!slash_turn_sets_busy("/skill:missing", &[], &skills));
    }
}

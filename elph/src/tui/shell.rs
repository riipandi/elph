//! Root shell: layout zones, global keyboard handling, and session state.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use elph_agent::{PromptTemplate, Skill};
use elph_tui::rgb;
use iocraft::prelude::*;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::agent::slash_commands_for_palette;
use crate::agent::{AgentUiEvent, CodingAgentSession, ToolApprovalChoice};
use crate::extensions::ExtensionHost;
use crate::platform::handle_prompt_interrupt_text;
use crate::platform::{Paths, PromptInterrupt};
use crate::types::{AgentMode, SlashCommand, ThinkingLevel};
use crate::types::{is_force_quit_command, is_quit_command};

use crate::tui::activity::TurnTokenTracker;
use crate::tui::activity::{
    accumulate_session_elapsed, activity_label_for_event, format_quit_canceled_notice,
    format_quit_while_busy_transcript, format_turn_canceled_notice, format_turn_complete_notice,
};
use crate::tui::agent_bridge::{PromptQueue, TranscriptEventApplier, TurnDispatcher};
use crate::tui::chrome::{ChromeStats, Header};
use crate::tui::chrome::{format_elapsed_secs, read_git_footer_info, refresh_chrome_stats};
use crate::tui::focus::ShellFocus;
use crate::tui::focus::{prompt_focus_char, shell_global_shortcut};

use crate::tui::prompt::PromptChrome;
use crate::tui::session_prefs::persist_session_prefs;
use crate::tui::slash_handler::{SlashContext, SlashOutcome};
use crate::tui::slash_handler::{handle_slash_submit, overlay_deferred_message};
use crate::tui::slash_palette::SlashPaletteKeyAction;
use crate::tui::slash_palette::{build_snapshot, resolve_snapshot_key_action, sync_selection};
use crate::tui::status_dialog::{StatusZone, build_status_dialog_kind};
use crate::tui::tool_approval::PendingToolApproval;
use crate::tui::tool_approval::{choice_at_index, pick_tool_approval_index_from_key};
use crate::tui::transcript::{TranscriptMessage, TranscriptPanel, TranscriptStyle};
use crate::tui::user_question::PendingUserQuestion;
use crate::tui::user_question::{
    QuestionInputFocus, StepNavOutcome, advance_question_selection, apply_step_nav_outcome, apply_step_submit_outcome,
    current_choice_index, is_custom_choice_index, navigate_step_delta, pick_step_tab_from_key,
    question_option_nav_delta, question_step_nav_delta, reset_ui_for_step, select_value_at, snapshot_current_answer,
    step_activity_label, try_resolve_submittable_answer,
};
use crate::tui::user_question_bar::{UserQuestionBar, UserQuestionView};
use elph_tui::components::ConfirmButtonFocus;

const SHELL_TICK_MS: u64 = 50;
const CHROME_REFRESH_TICKS: u32 = 20;
/// Cap transcript repaints during streaming so the prompt editor stays responsive.
const TRANSCRIPT_PUBLISH_MS: u64 = 66;
const MAX_UI_EVENTS_PER_TICK: usize = 64;
/// How long the status row shows turn elapsed after completion before returning to tips.
const TURN_COMPLETE_NOTICE_MS: u64 = 5_000;

struct IdleStatusNotice {
    text: String,
    since: Instant,
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
    pub bootstrap_notice: Option<String>,
    pub initial_agent_mode: AgentMode,
    pub initial_thinking_level: ThinkingLevel,
    pub model_label: String,
    pub project_label: String,
    pub context_limit: u64,
    pub supports_images: bool,
    pub footer_token_display: String,
    pub sticky_scroll: bool,
    pub show_thinking: bool,
    pub agent_session: Option<Arc<CodingAgentSession>>,
    pub ui_events: Option<Arc<Mutex<UnboundedReceiver<AgentUiEvent>>>>,
    pub extension_host: ExtensionHost,
    pub slash_commands: Vec<SlashCommand>,
    pub prompt_templates: Vec<PromptTemplate>,
    pub skills: Vec<Skill>,
    pub cwd: PathBuf,
}

impl Default for MainShellProps {
    fn default() -> Self {
        Self {
            session_id: "unavailable".to_string(),
            bootstrap_notice: None,
            initial_agent_mode: AgentMode::default(),
            initial_thinking_level: ThinkingLevel::default(),
            model_label: String::new(),
            project_label: String::new(),
            context_limit: 200_000,
            supports_images: false,
            footer_token_display: "both".to_string(),
            sticky_scroll: false,
            show_thinking: false,
            agent_session: None,
            ui_events: None,
            extension_host: ExtensionHost::new(),
            slash_commands: Vec::new(),
            prompt_templates: Vec::new(),
            skills: Vec::new(),
            cwd: PathBuf::new(),
        }
    }
}

struct BusyActivation<'a> {
    busy: &'a mut State<bool>,
    busy_started_at: &'a mut Ref<Option<Instant>>,
    activity_started_at: &'a mut Ref<Option<Instant>>,
    elapsed_secs: &'a mut State<f64>,
    activity_elapsed_secs: &'a mut State<f64>,
    spinner_tick: &'a mut State<u32>,
    activity_label: &'a mut State<String>,
    last_activity_label: &'a mut Ref<String>,
}

fn mark_busy(ctx: &mut BusyActivation<'_>, steer: bool) {
    let now = Instant::now();
    let label = if steer {
        "Steering".to_string()
    } else {
        "Thinking".to_string()
    };
    ctx.busy.set(true);
    ctx.busy_started_at.set(Some(now));
    ctx.activity_started_at.set(Some(now));
    ctx.elapsed_secs.set(0.0);
    ctx.activity_elapsed_secs.set(0.0);
    ctx.spinner_tick.set(0);
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

fn arm_pending_quit(
    pending_quit_confirm: &mut Ref<bool>,
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
) {
    if pending_quit_confirm.get() {
        return;
    }
    pending_quit_confirm.set(true);
    push_transcript_message(
        messages,
        messages_revision,
        TranscriptMessage::text(format_quit_while_busy_transcript(), TranscriptStyle::Meta),
    );
}

fn dismiss_pending_quit(
    pending_quit_confirm: &mut Ref<bool>,
    idle_status_notice: &mut Ref<Option<IdleStatusNotice>>,
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
) {
    if !pending_quit_confirm.get() {
        return;
    }
    pending_quit_confirm.set(false);
    idle_status_notice.set(Some(IdleStatusNotice {
        text: format_quit_canceled_notice(),
        since: Instant::now(),
    }));
    push_transcript_message(
        messages,
        messages_revision,
        TranscriptMessage::text(format_quit_canceled_notice(), TranscriptStyle::Meta),
    );
}

fn confirm_pending_quit(ctx: PendingQuitAction<'_>) {
    ctx.pending_quit_confirm.set(false);
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
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
    force: bool,
) -> bool {
    if force {
        confirm_pending_quit(ctx);
        return true;
    }
    if ctx.busy.get() {
        if ctx.pending_quit_confirm.get() {
            confirm_pending_quit(ctx);
            true
        } else {
            arm_pending_quit(ctx.pending_quit_confirm, messages, messages_revision);
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

#[component]
pub fn MainShell(props: &mut MainShellProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (screen_width, screen_height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut should_exit = hooks.use_state(|| false);
    let mut agent_mode = hooks.use_state(|| props.initial_agent_mode);
    let mut thinking_level = hooks.use_state(|| props.initial_thinking_level);
    let mut draft = hooks.use_state(String::new);
    let mut live_draft = hooks.use_ref(String::new);
    let bootstrap_notice = props.bootstrap_notice.clone();
    let mut messages = hooks.use_state(move || {
        bootstrap_notice
            .map(|notice| vec![TranscriptMessage::text(notice, TranscriptStyle::Meta)])
            .unwrap_or_default()
    });
    let mut messages_revision = hooks.use_state(|| 0u64);
    let mut suppress_enter_newline = hooks.use_ref(|| false);
    let mut slash_palette_active = hooks.use_ref(|| false);
    let mut force_palette_sync = hooks.use_ref(|| false);
    let mut force_editor_clear = hooks.use_ref(|| false);
    let mut busy = hooks.use_state(|| false);
    let mut activity_label = hooks.use_state(|| "Thinking".to_string());
    let mut elapsed_secs = hooks.use_state(|| 0.0f64);
    let mut activity_elapsed_secs = hooks.use_state(|| 0.0f64);
    let mut session_elapsed_secs = hooks.use_state(|| 0.0f64);
    let mut spinner_tick = hooks.use_state(|| 0u32);
    let show_thinking = props.show_thinking;
    let mut busy_started_at = hooks.use_ref(|| None::<Instant>);
    let mut activity_started_at = hooks.use_ref(|| None::<Instant>);
    let mut last_activity_label = hooks.use_ref(String::new);
    let mut prompt_queue = hooks.use_ref(PromptQueue::default);
    let mut event_applier = hooks.use_ref(|| TranscriptEventApplier::new(props.show_thinking));
    let mut pending_tool_approval = hooks.use_ref(|| None::<PendingToolApproval>);
    let mut pending_user_question = hooks.use_ref(|| None::<PendingUserQuestion>);
    let mut slash_commands = hooks.use_state(|| props.slash_commands.clone());
    let mut prompt_templates = hooks.use_state(|| props.prompt_templates.clone());
    let mut skills = hooks.use_state(|| props.skills.clone());
    let mut slash_palette_index = hooks.use_state(|| 0usize);
    let mut slash_palette_query = hooks.use_ref(String::new);
    let mut palette_refresh_pending = hooks.use_state(|| false);
    let mut shell_focus = hooks.use_state(ShellFocus::default);
    let mut question_selected = hooks.use_state(|| 0usize);
    let mut question_confirm_focus = hooks.use_state(ConfirmButtonFocus::default);
    let mut question_answer = hooks.use_state(String::new);
    let mut question_multi_checked = hooks.use_state(Vec::<bool>::new);
    let mut question_input_focus = hooks.use_state(QuestionInputFocus::default);
    let mut question_validation_error = hooks.use_state(|| None::<String>);
    let mut approval_selected = hooks.use_state(|| 0usize);

    let extension_host = props.extension_host.clone();
    let cwd = props.cwd.clone();

    let agent_session = props.agent_session.clone();
    let agent_session_for_loop = agent_session.clone();
    let agent_session_for_chrome = agent_session.clone();
    let agent_session_for_palette = agent_session.clone();
    let extension_host_for_palette = extension_host.clone();
    let ui_events = props.ui_events.clone();
    let paths = hooks.use_state(|| Paths::resolve().expect("resolve elph paths"));
    let mut skills_count = hooks.use_state(|| 0usize);
    let mut chrome_refresh_pending = hooks.use_state(|| true);
    let mut chrome_stats = hooks.use_state(|| ChromeStats {
        context_limit: props.context_limit,
        model_label: props.model_label.clone(),
        supports_images: props.supports_images,
        ..ChromeStats::default()
    });
    let mut git_footer = hooks.use_state(|| {
        let paths = Paths::resolve().expect("resolve elph paths");
        read_git_footer_info(paths.project_dir())
    });
    let mut chrome_tick = hooks.use_ref(|| 0u32);
    let fallback_context_limit = props.context_limit;
    let fallback_model_label = props.model_label.clone();
    let fallback_model_label_for_chrome = fallback_model_label.clone();
    let fallback_supports_images = props.supports_images;
    let footer_token_display = props.footer_token_display.clone();
    let session_id = props.session_id.clone();
    let mut transcript_pending = hooks.use_ref(|| false);
    let mut last_transcript_publish = hooks.use_ref(|| Instant::now() - Duration::from_millis(TRANSCRIPT_PUBLISH_MS));
    let mut idle_status_notice = hooks.use_ref(|| None::<IdleStatusNotice>);
    let mut turn_cancel_requested = hooks.use_ref(|| false);
    let mut pending_quit_confirm = hooks.use_ref(|| false);
    let mut turn_token_tracker = hooks.use_ref(|| None::<TurnTokenTracker>);

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(SHELL_TICK_MS)).await;

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
            if chrome_refresh_pending.get() || chrome_tick.get() % CHROME_REFRESH_TICKS == 0 {
                chrome_refresh_pending.set(false);
                let paths = paths.read().clone();
                git_footer.set(read_git_footer_info(paths.project_dir()));

                if let Some(session) = agent_session_for_chrome.as_ref() {
                    let resources = session.harness().get_resources().await;
                    skills_count.set(resources.skills.len());
                    let stats = refresh_chrome_stats(
                        Arc::clone(session),
                        fallback_context_limit,
                        &fallback_model_label_for_chrome,
                        fallback_supports_images,
                    )
                    .await;
                    chrome_stats.set(stats.clone());
                    if busy.get()
                        && let Some(tracker) = turn_token_tracker.write().as_mut()
                    {
                        tracker.sync_baseline(stats.tokens_used);
                    }
                }
            }

            if busy.get() {
                let current_label = activity_label.read().clone();
                if current_label != *last_activity_label.read() {
                    last_activity_label.set(current_label);
                    activity_started_at.set(Some(Instant::now()));
                    activity_elapsed_secs.set(0.0);
                }
                if let Some(started) = busy_started_at.read().as_ref() {
                    let next = format_elapsed_secs(*started);
                    if (elapsed_secs.get() - next).abs() > f64::EPSILON {
                        elapsed_secs.set(next);
                    }
                }
                if let Some(started) = activity_started_at.read().as_ref() {
                    let next = format_elapsed_secs(*started);
                    if (activity_elapsed_secs.get() - next).abs() > f64::EPSILON {
                        activity_elapsed_secs.set(next);
                    }
                }
                spinner_tick.set(spinner_tick.get().wrapping_add(1));
            }

            let idle_notice_expired = idle_status_notice
                .read()
                .as_ref()
                .is_some_and(|notice| notice.since.elapsed() >= Duration::from_millis(TURN_COMPLETE_NOTICE_MS));
            if idle_notice_expired {
                idle_status_notice.set(None);
            }

            let mut transcript_changed = false;
            let mut run_completed = false;
            let mut run_completed_elapsed: Option<f64> = None;

            if let Some(rx) = ui_events.as_ref()
                && let Ok(mut guard) = rx.lock()
            {
                let mut events_processed = 0usize;
                while events_processed < MAX_UI_EVENTS_PER_TICK {
                    let Ok(event) = guard.try_recv() else {
                        break;
                    };
                    events_processed += 1;
                    if !busy.get() && agent_event_keeps_busy(&event) {
                        mark_busy(
                            &mut BusyActivation {
                                busy: &mut busy,
                                busy_started_at: &mut busy_started_at,
                                activity_started_at: &mut activity_started_at,
                                elapsed_secs: &mut elapsed_secs,
                                activity_elapsed_secs: &mut activity_elapsed_secs,
                                spinner_tick: &mut spinner_tick,
                                activity_label: &mut activity_label,
                                last_activity_label: &mut last_activity_label,
                            },
                            false,
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

                    if let AgentUiEvent::Status(ref message) = event
                        && message.to_ascii_lowercase().contains("reloaded")
                    {
                        palette_refresh_pending.set(true);
                    }

                    if let AgentUiEvent::ToolApprovalRequired(req) = event {
                        let tool_name = req.tool_name.clone();
                        activity_label.set(format!("Approve: {tool_name}"));
                        approval_selected.set(0);
                        shell_focus.set(ShellFocus::StatusDialog);
                        pending_tool_approval.set(Some(PendingToolApproval::from_request(req)));
                        {
                            let mut msgs = messages.write();
                            msgs.push(TranscriptMessage::text(
                                format!("Tool approval required: {tool_name}"),
                                TranscriptStyle::Meta,
                            ));
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

            if transcript_changed {
                transcript_pending.set(true);
            }

            if transcript_pending.get()
                && (run_completed
                    || last_transcript_publish.get().elapsed().as_millis() >= TRANSCRIPT_PUBLISH_MS as u128)
            {
                messages_revision.set(messages_revision.get().wrapping_add(1));
                transcript_pending.set(false);
                last_transcript_publish.set(Instant::now());
            }

            if run_completed {
                pending_quit_confirm.set(false);
                if let Some(turn_elapsed) = run_completed_elapsed {
                    session_elapsed_secs.set(accumulate_session_elapsed(session_elapsed_secs.get(), turn_elapsed));
                }
                busy.set(false);
                busy_started_at.set(None);
                activity_started_at.set(None);
                elapsed_secs.set(0.0);
                activity_elapsed_secs.set(0.0);
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
                            elapsed_secs: &mut elapsed_secs,
                            activity_elapsed_secs: &mut activity_elapsed_secs,
                            spinner_tick: &mut spinner_tick,
                            activity_label: &mut activity_label,
                            last_activity_label: &mut last_activity_label,
                        },
                        false,
                    );
                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                    if let Some(session) = agent_session_for_loop.as_ref() {
                        chrome_refresh_pending.set(true);
                        TurnDispatcher::spawn_turn(Arc::clone(session), next, false);
                    }
                } else if turn_cancel_requested.get() {
                    turn_cancel_requested.set(false);
                    let elapsed = run_completed_elapsed.unwrap_or_else(|| elapsed_secs.get());
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

    hooks.use_terminal_events({
        let paths = paths.read().clone();
        let agent_session = agent_session.clone();
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

            let mut pending_tool_approval = pending_tool_approval;
            let mut pending_user_question = pending_user_question;
            let mut question_multi_checked = question_multi_checked;
            let mut question_input_focus = question_input_focus;
            let mut question_validation_error = question_validation_error;
            let mut pending_quit_confirm = pending_quit_confirm;
            if pending_quit_confirm.get() && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                match code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        confirm_pending_quit(PendingQuitAction {
                            pending_quit_confirm: &mut pending_quit_confirm,
                            should_exit: &mut should_exit,
                            busy: &busy,
                            turn_cancel_requested: &mut turn_cancel_requested,
                            prompt_queue: &mut prompt_queue,
                            pending_tool_approval: &mut pending_tool_approval,
                            pending_user_question: &mut pending_user_question,
                            agent_session: &agent_session,
                        });
                        return;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        dismiss_pending_quit(
                            &mut pending_quit_confirm,
                            &mut idle_status_notice,
                            &mut messages,
                            &mut messages_revision,
                        );
                        return;
                    }
                    _ => {}
                }
            }

            let status_dialog_open = pending_tool_approval.read().is_some() || pending_user_question.read().is_some();

            if status_dialog_open {
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
                        pending.respond(choice);
                    }
                    shell_focus.set(ShellFocus::Prompt);
                    activity_label.set(match choice {
                        ToolApprovalChoice::Approve => "Running approved tool…".to_string(),
                        ToolApprovalChoice::AllowSession => "Running tool (session allow)…".to_string(),
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

            let draft_text = live_draft.read().clone();
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
                        draft.set(completed.clone());
                        live_draft.set(completed);
                        suppress_enter_newline.set(suppress_enter);
                        force_palette_sync.set(true);
                        slash_palette_query.write().clear();
                        slash_palette_index.set(0);
                    }
                    SlashPaletteKeyAction::MoveSelection(index) => {
                        slash_palette_index.set(index);
                    }
                    SlashPaletteKeyAction::Dismiss => {
                        draft.set(String::new());
                        live_draft.set(String::new());
                        slash_palette_index.set(0);
                        suppress_enter_newline.set(true);
                    }
                }
                return;
            }

            if !status_dialog_open
                && shell_focus.get() == ShellFocus::Transcript
                && let Some(ch) = prompt_focus_char(code, modifiers)
            {
                shell_focus.set(ShellFocus::Prompt);
                let mut text = live_draft.read().clone();
                text.push(ch);
                draft.set(text.clone());
                live_draft.set(text);
                suppress_enter_newline.set(false);
                return;
            }

            match (modifiers, code) {
                (m, KeyCode::Esc) if m.is_empty() && shell_focus.get() == ShellFocus::Transcript => {
                    shell_focus.set(ShellFocus::Prompt);
                }
                (m, KeyCode::Tab) if m.is_empty() && !status_dialog_open => {
                    let next = agent_mode.get().next();
                    agent_mode.set(next);
                    persist_session_prefs(&paths, next, thinking_level.get());
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
                (m, KeyCode::BackTab) if m.contains(KeyModifiers::SHIFT) => {
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
                (m, KeyCode::Char('d')) if m.contains(KeyModifiers::CONTROL) => {
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
                        &mut messages,
                        &mut messages_revision,
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
                    if let Some(session) = agent_session.as_ref() {
                        TurnDispatcher::spawn_abort(Arc::clone(session));
                    } else {
                        let canceled_elapsed = busy_started_at
                            .read()
                            .as_ref()
                            .map(|started| format_elapsed_secs(*started))
                            .unwrap_or(elapsed_secs.get());
                        session_elapsed_secs
                            .set(accumulate_session_elapsed(session_elapsed_secs.get(), canceled_elapsed));
                        busy.set(false);
                        busy_started_at.set(None);
                        activity_started_at.set(None);
                        elapsed_secs.set(0.0);
                        activity_elapsed_secs.set(0.0);
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
        system.exit();
    }

    let (accent_r, accent_g, accent_b) = agent_mode.get().label_rgb();
    let scanner_accent = rgb(accent_r, accent_g, accent_b);
    let chrome = chrome_stats.read();
    let mcp_connected = agent_session
        .as_ref()
        .and_then(|session| session.mcp_registry())
        .map(|registry| registry.load_report().servers_ok)
        .unwrap_or(0);
    let project_name = paths
        .read()
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
    let status_dialog_open = pending_tool_approval.read().is_some() || user_question_open;
    let prompt_focused =
        !status_dialog_open && matches!(shell_focus.get(), ShellFocus::Prompt | ShellFocus::StatusDialog);
    let transcript_focused = !status_dialog_open && shell_focus.get() == ShellFocus::Transcript;
    let question_has_focus = user_question_open;
    let approval_has_focus = pending_tool_approval.read().is_some() && !user_question_open;
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
    let draft_for_palette = live_draft.read().clone();
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
                messages_revision: messages_revision.get(),
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
                spinner_tick: spinner_tick.get(),
                activity_elapsed_secs: activity_elapsed_secs.get(),
                turn_elapsed_secs: live_turn_elapsed_secs(busy.get(), &busy_started_at.read()),
                session_elapsed_secs: session_elapsed_secs.get(),
                idle_notice: idle_status_notice.read().as_ref().map(|notice| notice.text.clone()),
                quit_confirm_pending: pending_quit_confirm.get(),
                dialog: status_dialog,
                approval_selected: Some(approval_selected),
                approval_has_focus: approval_has_focus,
            )
            PromptChrome(
                screen_width: screen_width,
                screen_height: screen_height,
                agent_mode: agent_mode.get(),
                thinking_level: thinking_level.get(),
                has_focus: prompt_focused,
                project_name: project_name.clone(),
                git: git.clone(),
                turn: chrome.turn_count,
                model_label: model_label.clone(),
                supports_images: supports_images,
                draft: Some(draft),
                live_draft: Some(live_draft),
                suppress_enter_newline: Some(suppress_enter_newline),
                slash_palette_active: Some(slash_palette_active),
                force_palette_sync: Some(force_palette_sync),
                force_editor_clear: Some(force_editor_clear),
                slash_palette_snapshot: slash_palette_snapshot,
                slash_palette_selected: Some(slash_palette_index),
                blocked_hint: user_question_open.then(|| "Answer the question above".to_string()),
                on_escape: move |_| {
                    shell_focus.set(ShellFocus::Transcript);
                },
                on_submit: move |text: String| {
                        shell_focus.set(ShellFocus::Prompt);
                        if is_force_quit_command(&text) || is_quit_command(&text) {
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
                                &mut messages,
                                &mut messages_revision,
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

                        let is_slash = text.trim_start().starts_with('/');
                        let mut submitted = TranscriptMessage::text(
                            text.clone(),
                            TranscriptStyle::for_user_submit(&text),
                        );
                        if submitted.style.is_user_input_card() {
                            submitted.submitted_at = Some(chrono::Utc::now());
                        }
                        push_transcript_message(&mut messages, &mut messages_revision, submitted);

                        let extension_registry = extension_host.registry();
                        let ext_registry = extension_registry.read();
                        let templates = prompt_templates.read().clone();
                        let loaded_skills = skills.read().clone();
                        let paths_snapshot = paths.read().clone();
                        let outcome = handle_slash_submit(SlashContext {
                            input: &text,
                            extensions: Some(&ext_registry),
                            prompt_templates: Some(&templates),
                            skills: Some(&loaded_skills),
                            agent_session: agent_session.clone(),
                            extension_host: Some(&extension_host),
                            paths: Some(&paths_snapshot),
                            cwd: Some(&cwd),
                        });

                        match outcome {
                            SlashOutcome::Quit => {
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
                                    &mut messages,
                                    &mut messages_revision,
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
                            SlashOutcome::Unimplemented(message) => {
                                push_transcript_message(
                                    &mut messages,
                                    &mut messages_revision,
                                    TranscriptMessage::text(message, TranscriptStyle::Meta),
                                );
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
                                            elapsed_secs: &mut elapsed_secs,
                                            activity_elapsed_secs: &mut activity_elapsed_secs,
                                            spinner_tick: &mut spinner_tick,
                                            activity_label: &mut activity_label,
                                            last_activity_label: &mut last_activity_label,
                                        },
                                        false,
                                    );
                                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                                }
                            }
                            SlashOutcome::SpawnAgentTurn => {
                                if busy.get() {
                                    prompt_queue.write().push(text);
                                } else if let Some(session) = agent_session.as_ref() {
                                    chrome_refresh_pending.set(true);
                                    idle_status_notice.set(None);
                                    turn_cancel_requested.set(false);
                                    mark_busy(
                                        &mut BusyActivation {
                                            busy: &mut busy,
                                            busy_started_at: &mut busy_started_at,
                                            activity_started_at: &mut activity_started_at,
                                            elapsed_secs: &mut elapsed_secs,
                                            activity_elapsed_secs: &mut activity_elapsed_secs,
                                            spinner_tick: &mut spinner_tick,
                                            activity_label: &mut activity_label,
                                            last_activity_label: &mut last_activity_label,
                                        },
                                        false,
                                    );
                                    begin_turn_token_tracking(&mut turn_token_tracker, &chrome_stats.read());
                                    TurnDispatcher::spawn_turn(Arc::clone(session), text, false);
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

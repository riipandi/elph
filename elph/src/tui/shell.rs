//! Root shell: layout zones, global keyboard handling, and session state.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use elph_agent::{PromptTemplate, Skill};
use elph_tui::rgb;
use iocraft::prelude::*;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::agent::{
    AgentUiEvent, CodingAgentSession, ToolApprovalChoice, parse_skill_slash, slash_commands_for_palette,
};
use crate::extensions::ExtensionHost;
use crate::platform::{Paths, PromptInterrupt, handle_prompt_interrupt_text};
use crate::types::{AgentMode, SlashCommand, ThinkingLevel, is_quit_command};

use crate::tui::activity::{
    TurnTokenTracker, activity_label_for_event, format_busy_token_info, format_turn_canceled_notice,
    format_turn_complete_notice,
};
use crate::tui::agent_bridge::{PromptQueue, TranscriptEventApplier, TurnDispatcher};
use crate::tui::chrome::{ChromeStats, Header, StatusRow};
use crate::tui::chrome::{format_elapsed_secs, header_stats_from_chrome, read_git_branch, refresh_chrome_stats};
use crate::tui::focus::{ShellFocus, prompt_focus_char};
use crate::tui::labels::{footer_left_label, project_footer_label, session_label};
use crate::tui::prompt::{Footer, PromptChrome};
use crate::tui::session_prefs::persist_session_prefs;
use crate::tui::slash_handler::{SlashContext, SlashOutcome, handle_slash_submit, overlay_deferred_message};
use crate::tui::slash_palette::{SlashPaletteKeyAction, build_snapshot, resolve_snapshot_key_action, sync_selection};
use crate::tui::tool_approval::{PendingToolApproval, ToolApprovalPrompt, choice_from_key};
use crate::tui::transcript::{TranscriptMessage, TranscriptPanel, TranscriptStyle};
use crate::tui::user_question::{PendingUserQuestion, UserQuestionPrompt, confirm_from_key, option_index_from_key};

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
    elapsed_secs: &'a mut State<f64>,
    spinner_tick: &'a mut State<u32>,
    activity_label: &'a mut State<String>,
}

fn mark_busy(ctx: &mut BusyActivation<'_>, steer: bool) {
    ctx.busy.set(true);
    ctx.busy_started_at.set(Some(Instant::now()));
    ctx.elapsed_secs.set(0.0);
    ctx.spinner_tick.set(0);
    ctx.activity_label.set(if steer {
        "Steering".to_string()
    } else {
        "Thinking".to_string()
    });
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
    let mut spinner_tick = hooks.use_state(|| 0u32);
    let show_thinking = props.show_thinking;
    let mut busy_started_at = hooks.use_ref(|| None::<Instant>);
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
    let mut project_label = hooks.use_state(|| props.project_label.clone());
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
                let branch = read_git_branch(paths.project_dir());
                project_label.set(project_footer_label(&paths, branch.as_deref()));

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
                if let Some(started) = busy_started_at.read().as_ref() {
                    let next = format_elapsed_secs(*started);
                    if (elapsed_secs.get() - next).abs() > f64::EPSILON {
                        elapsed_secs.set(next);
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
                        pending_tool_approval.set(Some(PendingToolApproval::from_request(req)));
                        {
                            let mut msgs = messages.write();
                            msgs.push(TranscriptMessage::text(
                                format!("Tool approval required: {tool_name} (y/n/a)"),
                                TranscriptStyle::Meta,
                            ));
                        }
                        transcript_changed = true;
                        continue;
                    }

                    if let AgentUiEvent::UserQuestionRequired(req) = event {
                        activity_label.set("Awaiting your answer".to_string());
                        pending_user_question.set(Some(PendingUserQuestion::from_request(req)));
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
                busy.set(false);
                busy_started_at.set(None);
                elapsed_secs.set(0.0);
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
                            elapsed_secs: &mut elapsed_secs,
                            spinner_tick: &mut spinner_tick,
                            activity_label: &mut activity_label,
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
            if pending_tool_approval.read().is_some()
                && let Some(choice) = choice_from_key(modifiers, code)
            {
                if let Some(pending) = pending_tool_approval.write().take() {
                    pending.respond(choice);
                }
                activity_label.set(match choice {
                    ToolApprovalChoice::Approve => "Running approved tool…".to_string(),
                    ToolApprovalChoice::AllowSession => "Running tool (session allow)…".to_string(),
                    ToolApprovalChoice::Reject => "Tool denied".to_string(),
                });
                return;
            }

            let user_question_confirm = pending_user_question
                .read()
                .as_ref()
                .is_some_and(|pending| pending.is_confirm)
                .then(|| confirm_from_key(modifiers, code))
                .flatten();
            if let Some(yes) = user_question_confirm {
                if let Some(question) = pending_user_question.write().take() {
                    question.respond_confirm(yes);
                }
                activity_label.set("Thinking".to_string());
                return;
            }

            let user_question_option = pending_user_question.read().as_ref().and_then(|pending| {
                let index = option_index_from_key(modifiers, code)?;
                pending
                    .options
                    .as_ref()?
                    .get(index.saturating_sub(1))
                    .map(|option| option.value.clone())
            });
            if let Some(value) = user_question_option {
                if let Some(question) = pending_user_question.write().take() {
                    question.respond_option(value);
                }
                activity_label.set("Thinking".to_string());
                return;
            }

            let draft_text = live_draft.read().clone();
            let palette_snapshot = build_snapshot(&draft_text, &slash_commands.read(), screen_height);
            if let Some(action) =
                resolve_snapshot_key_action(&draft_text, &palette_snapshot, slash_palette_index.get(), code, modifiers)
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

            if shell_focus.get() == ShellFocus::Transcript
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
                (m, KeyCode::Tab) if m.is_empty() => {
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
                (m, KeyCode::Char('d')) if m.contains(KeyModifiers::CONTROL) => should_exit.set(true),
                (m, KeyCode::Char('c')) if m.contains(KeyModifiers::CONTROL) && busy.get() => {
                    turn_cancel_requested.set(true);
                    activity_label.set("Cancelling…".to_string());
                    prompt_queue.write().clear();
                    if let Some(pending) = pending_tool_approval.write().take() {
                        pending.respond(ToolApprovalChoice::Reject);
                    }
                    if let Some(question) = pending_user_question.write().take() {
                        question.respond(String::new());
                    }
                    if let Some(session) = agent_session.as_ref() {
                        TurnDispatcher::spawn_abort(Arc::clone(session));
                    } else {
                        let canceled_elapsed = busy_started_at
                            .read()
                            .as_ref()
                            .map(|started| format_elapsed_secs(*started))
                            .unwrap_or(elapsed_secs.get());
                        busy.set(false);
                        busy_started_at.set(None);
                        elapsed_secs.set(0.0);
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
    let session_label = session_label(&session_id, mcp_connected, skills_count.get());
    let footer_left = footer_left_label(&project_label.read(), chrome.turn_count);
    let stats_label = header_stats_from_chrome(&chrome, &footer_token_display);
    let model_label = if chrome.model_label.is_empty() {
        fallback_model_label.clone()
    } else {
        chrome.model_label.clone()
    };
    let supports_images = chrome.supports_images;
    let prompt_focused = shell_focus.get() == ShellFocus::Prompt;
    let transcript_focused = shell_focus.get() == ShellFocus::Transcript;
    let busy_token_info = if busy.get() {
        turn_token_tracker
            .read()
            .as_ref()
            .map(|tracker| format_busy_token_info(tracker.stream_tokens, tracker.tokens_per_sec(elapsed_secs.get())))
    } else {
        None
    };

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
                session_label: session_label,
                stats_label: stats_label,
            )
            TranscriptPanel(
                screen_width: screen_width,
                messages: Some(messages),
                messages_revision: messages_revision.get(),
                sticky_scroll: props.sticky_scroll,
                has_focus: transcript_focused,
            )
            StatusRow(
                screen_width: screen_width,
                busy: busy.get(),
                activity_label: activity_label.read().clone(),
                accent: scanner_accent,
                spinner_tick: spinner_tick.get(),
                elapsed_secs: elapsed_secs.get(),
                idle_notice: idle_status_notice.read().as_ref().map(|notice| notice.text.clone()),
                busy_token_info: busy_token_info.clone(),
            )
            #(if let Some(pending) = pending_tool_approval.read().as_ref() {
                element! {
                    View(
                        width: screen_width,
                        flex_shrink: 0f32,
                        flex_direction: FlexDirection::Column,
                    ) {
                        ToolApprovalPrompt(
                            screen_width: screen_width,
                            tool_name: pending.tool_name.clone(),
                            args_summary: pending.args_summary.clone(),
                        )
                        Footer(
                            screen_width: screen_width,
                            project_label: footer_left.clone(),
                            model_label: model_label.clone(),
                            thinking_level: thinking_level.get(),
                            supports_images: supports_images,
                        )
                    }
                }
            } else if let Some(question) = pending_user_question.read().as_ref()
                && !question.needs_text_input() {
                element! {
                    View(
                        width: screen_width,
                        flex_shrink: 0f32,
                        flex_direction: FlexDirection::Column,
                    ) {
                        UserQuestionPrompt(
                            screen_width: screen_width,
                            question: question.question.clone(),
                            options: question.options.clone(),
                            is_confirm: question.is_confirm,
                            needs_text_input: false,
                        )
                        Footer(
                            screen_width: screen_width,
                            project_label: footer_left.clone(),
                            model_label: model_label.clone(),
                            thinking_level: thinking_level.get(),
                            supports_images: supports_images,
                        )
                    }
                }
            } else {
                element! {
                    View(
                        width: screen_width,
                        flex_shrink: 0f32,
                        flex_direction: FlexDirection::Column,
                    ) {
                        #(pending_user_question.read().as_ref().filter(|question| question.needs_text_input()).map(|question| -> AnyElement<'static> {
                            element! {
                                UserQuestionPrompt(
                                    screen_width: screen_width,
                                    question: question.question.clone(),
                                    options: None,
                                    is_confirm: false,
                                    needs_text_input: true,
                                )
                            }.into()
                        }))
                        PromptChrome(
                        screen_width: screen_width,
                        screen_height: screen_height,
                        agent_mode: agent_mode.get(),
                        thinking_level: thinking_level.get(),
                        has_focus: prompt_focused,
                        project_label: footer_left.clone(),
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
                        on_escape: move |_| {
                            shell_focus.set(ShellFocus::Transcript);
                        },
                        on_submit: move |text: String| {
                    shell_focus.set(ShellFocus::Prompt);
                    if let Some(question) = pending_user_question.write().take() {
                        let answer = if text.trim().is_empty() {
                            question.default.clone().unwrap_or_default()
                        } else {
                            text
                        };
                        question.respond(answer);
                        draft.set(String::new());
                        live_draft.set(String::new());
                        suppress_enter_newline.set(true);
                        activity_label.set("Thinking".to_string());
                        return;
                    }
                    if text.trim().is_empty() {
                        return;
                    }
                    if is_quit_command(&text) {
                        should_exit.set(true);
                        draft.set(String::new());
                        live_draft.set(String::new());
                        suppress_enter_newline.set(true);
                        return;
                    }

                    let is_slash = text.trim_start().starts_with('/');
                    push_transcript_message(
                        &mut messages,
                        &mut messages_revision,
                        TranscriptMessage::text(text.clone(), TranscriptStyle::for_user_submit(&text)),
                    );

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
                        SlashOutcome::Quit => should_exit.set(true),
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
                            if agent_session.is_some() && slash_turn_sets_busy(&text, &templates, &loaded_skills) {
                                chrome_refresh_pending.set(true);
                                idle_status_notice.set(None);
                                turn_cancel_requested.set(false);
                                mark_busy(
                                    &mut BusyActivation {
                                        busy: &mut busy,
                                        busy_started_at: &mut busy_started_at,
                                        elapsed_secs: &mut elapsed_secs,
                                        spinner_tick: &mut spinner_tick,
                                        activity_label: &mut activity_label,
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
                                        elapsed_secs: &mut elapsed_secs,
                                        spinner_tick: &mut spinner_tick,
                                        activity_label: &mut activity_label,
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
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elph_agent::Skill;

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

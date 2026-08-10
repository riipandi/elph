//! Internal shell helper logic shared by the component, tick loop, key handler, and view.

use super::*;

pub(crate) fn initial_layout_screen_size() -> (u16, u16) {
    crossterm::terminal::size()
        .map(|(width, height)| (width.max(1), height.max(1)))
        .unwrap_or((FALLBACK_TERMINAL_WIDTH, FALLBACK_TERMINAL_HEIGHT))
}

pub(crate) fn merge_layout_screen_size(layout_size: &mut State<(u16, u16)>, hook_width: u16, hook_height: u16) {
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

/// Debounced terminal-size polling.
///
/// During a resize drag, the terminal reports a new size every frame (~50 ms).
/// Applying each change immediately triggers a full transcript re-measure + re-render
/// (including expensive mermaid renders at every intermediate width), causing visible
/// lag. Instead, we record the latest pending size and only commit it after
/// `DEBOUNCE_MS` of no further changes — coalescing the rapid stream of resize events
/// into a single update when the user "settles" on a size.
static PENDING_SIZE: std::sync::OnceLock<Mutex<(u16, u16, u64)>> = std::sync::OnceLock::new();

/// How long to wait after the last resize event before applying the new size.
const RESIZE_DEBOUNCE_TICKS: u64 = 3; // ~150 ms at 50 ms/tick

fn pending_size() -> &'static Mutex<(u16, u16, u64)> {
    PENDING_SIZE.get_or_init(|| Mutex::new((0, 0, 0)))
}

pub(crate) fn poll_layout_screen_size(layout_size: &mut State<(u16, u16)>) {
    if let Ok((width, height)) = crossterm::terminal::size() {
        let next = (width.max(1), height.max(1));
        let current = layout_size.get();

        if next == current {
            // Size matches committed state — clear any pending debounce.
            if let Ok(mut pending) = pending_size().lock() {
                pending.2 = 0;
            }
            return;
        }

        // Size changed — record it and start/continue debounce countdown.
        if let Ok(mut pending) = pending_size().lock() {
            let tick = pending.2.saturating_add(1);
            *pending = (next.0, next.1, tick);

            if tick >= RESIZE_DEBOUNCE_TICKS {
                // Debounced long enough — commit the new size.
                layout_size.set(next);
                pending.2 = 0;
            }
        } else {
            // Lock failed — apply immediately as fallback.
            layout_size.set(next);
        }
    }
}

pub(crate) fn bump_chrome_ui_revision(chrome_ui_revision: &mut State<u64>) {
    chrome_ui_revision.set(chrome_ui_revision.get().wrapping_add(1));
}

pub(crate) fn thinking_level_from_agent(level: elph_agent::AgentThinkingLevel) -> ThinkingLevel {
    crate::agent::from_agent_thinking(level)
}

pub(crate) async fn restored_thinking_level_for_session(session: &Arc<CodingAgentSession>) -> ThinkingLevel {
    thinking_level_from_agent(session.harness().get_thinking_level().await)
}

/// Publish chrome stats when they change. Returns `true` if values were updated.
///
/// Callers that need a footer/header repaint even when values are unchanged
/// (AgentReady, first layout paint) should call `bump_chrome_ui_revision` when this returns `false`.
pub(crate) fn publish_chrome_stats(
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

pub(crate) struct IdleStatusNotice {
    pub(crate) text: String,
    pub(crate) since: Instant,
}

/// Shared output buffers for real-time subagent dialog display.
/// Maps agent_id → (text, is_running).
pub(crate) type SubagentBuf = (Arc<RwLock<String>>, Arc<std::sync::atomic::AtomicBool>);

/// Channel for pushing user-shell events to the shell loop.
pub(crate) struct UserShellChannel {
    pub(crate) tx: UnboundedSender<UserShellEvent>,
    pub(crate) rx: UnboundedReceiver<UserShellEvent>,
}

/// Channel for scheduling auto-clear of ephemeral banners.
pub(crate) struct EphemeralExpireChannel {
    pub(crate) tx: UnboundedSender<u64>,
    pub(crate) rx: UnboundedReceiver<u64>,
}

pub(crate) fn count_submitted_user_prompts(messages: &[TranscriptMessage]) -> u32 {
    messages
        .iter()
        .filter(|message| {
            message.style.is_user_input_card() && message.submitted_at.is_some() && !message.content.trim().is_empty()
        })
        .count() as u32
}

pub(crate) fn live_turn_elapsed_secs(busy: bool, busy_started_at: &Option<Instant>) -> f64 {
    if !busy {
        return 0.0;
    }
    busy_started_at
        .as_ref()
        .map(|started| format_elapsed_secs(*started))
        .unwrap_or(0.0)
}

pub(crate) fn agent_event_keeps_busy(event: &AgentUiEvent) -> bool {
    matches!(
        event,
        AgentUiEvent::TextDelta(_)
            | AgentUiEvent::ThinkingDelta(_)
            | AgentUiEvent::ToolStart { .. }
            | AgentUiEvent::ToolUpdate { .. }
            | AgentUiEvent::ToolEnd { .. }
            | AgentUiEvent::SubagentStatus { .. }
            | AgentUiEvent::SubagentOutput { .. }
            | AgentUiEvent::Retrying { .. }
    )
}

pub(crate) struct BusyActivation<'a> {
    pub(crate) busy: &'a mut State<bool>,
    pub(crate) busy_started_at: &'a mut Ref<Option<Instant>>,
    pub(crate) activity_started_at: &'a mut Ref<Option<Instant>>,
    pub(crate) activity_label: &'a mut State<String>,
    pub(crate) last_activity_label: &'a mut Ref<String>,
}

pub(crate) fn mark_busy(ctx: &mut BusyActivation<'_>, steer: bool, activity_label: Option<&str>) {
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

/// Slim transcript label for a worker-message turn prompt.
///
/// Extracts the sender name from the `<intercom> … (\`name\`) prefix and a
/// short preview of the actual message. Never shows the raw `<intercom>`
/// wrapper or the full peer message as a prompt card.
pub(crate) fn worker_inbound_meta_label(prompt: &str) -> String {
    let after = prompt
        .strip_prefix(crate::agent::WORKER_INBOUND_PROMPT_PREFIX)
        .unwrap_or(prompt);
    // Sender: ` (`name`)`  right after the prefix.
    let mut sender = String::new();
    let after_sender = after
        .strip_prefix(" (`")
        .and_then(|s| {
            let (name, tail) = s.split_once("`)\n")?;
            sender = name.trim().to_string();
            Some(tail)
        })
        .unwrap_or(after);
    // Body: everything after the closing `</intercom>`.
    let body = after_sender
        .split_once("</intercom>")
        .map(|(_, tail)| tail)
        .unwrap_or(after_sender);
    let body = body.trim();
    let preview: String = body.chars().take(60).collect();
    if sender.is_empty() {
        format!("Message from worker… {preview}")
    } else if preview.is_empty() {
        format!("Message from worker {sender}")
    } else {
        format!("Message from worker {sender} — {preview}")
    }
}

/// Mutable UI state for queue manager actions (grouped for clippy::too_many_arguments).
pub(crate) struct PromptQueueActionCtx<'a> {
    pub(crate) prompt_queue: &'a mut Ref<PromptQueue>,
    pub(crate) queue_ui_revision: &'a mut State<u64>,
    pub(crate) agent_session: &'a Option<Arc<CodingAgentSession>>,
    pub(crate) agent_turn_active: bool,
    pub(crate) messages: &'a mut State<Vec<TranscriptMessage>>,
    pub(crate) messages_revision: &'a mut State<u64>,
    pub(crate) prompt_history: &'a mut Ref<Vec<String>>,
    pub(crate) pre_echoed_user_prompts: &'a mut State<u32>,
    pub(crate) draft: &'a mut State<String>,
    pub(crate) live_draft: &'a mut Ref<String>,
    pub(crate) live_cursor: &'a mut Ref<usize>,
    pub(crate) prompt_editor_mirror: &'a mut Ref<(String, usize)>,
    pub(crate) force_palette_sync: &'a mut Ref<bool>,
    pub(crate) shell_focus: &'a mut State<ShellFocus>,
    pub(crate) queue_manager_open: &'a mut State<bool>,
    pub(crate) queue_manager_selected: &'a mut State<usize>,
    pub(crate) queue_manager_action: &'a mut State<PromptQueueAction>,
    /// Shared arc for transcript messages — pre-echoed prompts are written here too
    /// so the arc-to-state sync never loses them.
    pub(crate) messages_arc: &'a mut Ref<Arc<RwLock<Vec<TranscriptMessage>>>>,
}

/// Close the queue manager and return focus to the prompt.
pub(crate) fn close_queue_manager(ctx: &mut PromptQueueActionCtx<'_>) {
    ctx.queue_manager_open.set(false);
    ctx.queue_manager_selected.set(0);
    ctx.queue_manager_action.set(PromptQueueAction::SendNow);
    ctx.shell_focus.set(ShellFocus::Prompt);
}

/// Apply Send / Edit / Cancel on a queue row.
///
/// - **Send** — drop from queue and interject (steer); when idle, steer falls back to a normal turn.
/// - **Edit** — drop from queue and load the text into the prompt editor.
/// - **Cancel** — drop from queue only.
///
/// Returns `Some(text)` when the shell should mark a busy idle turn (Send while not already streaming).
pub(crate) fn apply_prompt_queue_action(
    action: PromptQueueAction,
    display_index: usize,
    ctx: &mut PromptQueueActionCtx<'_>,
) -> Option<String> {
    let item = ctx.prompt_queue.read().items().get(display_index).cloned()?;
    // Optimistic local remove so the row disappears immediately; harness QueueUpdate reconciles.
    let _ = ctx.prompt_queue.write().remove_at_local(display_index);
    ctx.queue_ui_revision.set(ctx.queue_ui_revision.get().wrapping_add(1));

    match action {
        PromptQueueAction::SendNow => {
            // Hide immediately and keep hidden if interject re-queues as steer.
            ctx.prompt_queue.write().suppress_sent(item.text.clone());
            ctx.queue_ui_revision.set(ctx.queue_ui_revision.get().wrapping_add(1));
            let mut submitted = TranscriptMessage::text(item.text.clone(), TranscriptStyle::User);
            submitted.submitted_at = Some(chrono::Utc::now());
            // Sync to shared arc so the arc-to-state sync doesn't lose this pre-echoed prompt.
            ctx.messages_arc.write().write().unwrap().push(submitted.clone());
            push_transcript_message(ctx.messages, ctx.messages_revision, ctx.prompt_history, submitted);
            ctx.pre_echoed_user_prompts
                .set(ctx.pre_echoed_user_prompts.get().saturating_add(1));
            let need_busy = !ctx.agent_turn_active;
            if let Some(session) = ctx.agent_session.as_ref() {
                // Always interject: remove from harness queue then steer (idle → normal turn).
                TurnDispatcher::spawn_interject_queued(
                    Arc::clone(session),
                    item.kind,
                    item.kind_index,
                    item.text.clone(),
                );
            }
            close_queue_manager(ctx);
            if need_busy { Some(item.text) } else { None }
        }
        PromptQueueAction::Edit => {
            if let Some(session) = ctx.agent_session.as_ref() {
                TurnDispatcher::spawn_remove_queued(Arc::clone(session), item.kind, item.kind_index);
            }
            let text = item.text;
            let cursor = text.len();
            ctx.draft.set(text.clone());
            ctx.live_draft.set(text.clone());
            ctx.live_cursor.set(cursor);
            ctx.prompt_editor_mirror.set((text, cursor));
            // Force Textarea to accept the external draft (focused sync is prefix-only).
            ctx.force_palette_sync.set(true);
            close_queue_manager(ctx);
            None
        }
        PromptQueueAction::Cancel => {
            if let Some(session) = ctx.agent_session.as_ref() {
                TurnDispatcher::spawn_remove_queued(Arc::clone(session), item.kind, item.kind_index);
            }
            let len = ctx.prompt_queue.read().len();
            if len == 0 {
                close_queue_manager(ctx);
            } else {
                ctx.queue_manager_selected.set(display_index.min(len - 1));
            }
            None
        }
    }
}

pub(crate) struct PendingQuitAction<'a> {
    pub(crate) pending_quit_confirm: &'a mut Ref<bool>,
    pub(crate) should_exit: &'a mut State<bool>,
    pub(crate) busy: &'a State<bool>,
    pub(crate) turn_cancel_requested: &'a mut Ref<bool>,
    pub(crate) prompt_queue: &'a mut Ref<PromptQueue>,
    pub(crate) pending_tool_approval: &'a mut Ref<Option<PendingToolApproval>>,
    pub(crate) pending_user_question: &'a mut Ref<Option<PendingUserQuestion>>,
    pub(crate) agent_session: &'a Option<Arc<CodingAgentSession>>,
}

/// Show a fixed toast above StatusRow. Timed banners schedule an async clear that does **not**
/// wait for agent busy/stream to finish; generation guards ignore stale clear tasks.
pub(crate) fn show_ephemeral_banner(
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

pub(crate) fn clear_quit_busy_banner(
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

pub(crate) fn poll_ephemeral_banner_expiry(
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

pub(crate) fn arm_pending_quit(
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

pub(crate) fn dismiss_pending_quit(
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

pub(crate) fn confirm_pending_quit(
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
            // Abort clears harness steer/follow-up queues.
            TurnDispatcher::spawn_abort(Arc::clone(session));
        }
    }
    // Graceful worker teardown before exit (bounded wait so quit still responds).
    await_worker_shutdown(ctx.agent_session.as_ref());
    ctx.should_exit.set(true);
}

/// Request application exit. Returns `true` when the shell should exit now.
pub(crate) fn request_quit(
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
        await_worker_shutdown(ctx.agent_session.as_ref());
        ctx.should_exit.set(true);
        true
    }
}

/// Best-effort: run `shutdown_workers` and wait up to 2s (multi-thread runtime).
fn await_worker_shutdown(session: Option<&Arc<CodingAgentSession>>) {
    let Some(session) = session.map(Arc::clone) else {
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    handle.spawn(async move {
        session.shutdown_workers().await;
        let _ = tx.send(());
    });
    let _ = rx.recv_timeout(std::time::Duration::from_secs(2));
}

/// Blocking best-effort delete of an empty session (no turns). Used from the
/// synchronous render path where `await` is not available. Bounded wait so the
/// TUI exit is never blocked indefinitely.
pub(crate) fn delete_empty_session_blocking(session: Arc<crate::agent::CodingAgentSession>, session_id: &str) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let sid = session_id.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    handle.spawn(async move {
        let result = session.session_manager().delete_if_no_turns(&sid).await;
        let _ = tx.send(result);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(deleted)) if deleted => {
            log::debug!("deleted empty session {session_id} on exit");
        }
        Ok(Ok(_)) => {}
        Ok(Err(err)) => log::warn!("delete empty session on exit: {err:#}"),
        Err(_) => log::warn!("delete empty session on exit: timed out"),
    }
}

pub(crate) fn begin_turn_token_tracking(tracker: &mut Ref<Option<TurnTokenTracker>>, chrome: &ChromeStats) {
    tracker.set(Some(TurnTokenTracker::new(chrome.tokens_used)));
}

pub(crate) fn push_transcript_message(
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
    prompt_history: &mut Ref<Vec<String>>,
    message: TranscriptMessage,
) {
    // Keep Arrow Up history in sync with user / skill prompt cards.
    // Skills → `/skill:…`; other slash commands keep a leading `/`.
    if matches!(message.style, TranscriptStyle::User | TranscriptStyle::SkillPrompt) {
        crate::tui::prompt_history::push_history_entry_styled(
            &mut prompt_history.write(),
            &message.content,
            message.style,
        );
    }
    messages.set({
        let mut list = messages.read().clone();
        list.push(message);
        list
    });
    messages_revision.set(messages_revision.get().wrapping_add(1));
}

/// Push a transcript message to both the shared arc and the messages State.
///
/// The arc-to-state sync (`*messages.write() = messages_arc_inner.read()...`)
/// overwrites the State with the arc content. Messages written only to the
/// State (slash output, notices) would be lost on the next sync, so they must
/// also be written to the shared arc.
pub(crate) fn push_transcript_message_synced(
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_arc: Ref<Arc<RwLock<Vec<TranscriptMessage>>>>,
    messages_revision: &mut State<u64>,
    prompt_history: &mut Ref<Vec<String>>,
    message: TranscriptMessage,
) {
    let mut arc = messages_arc;
    arc.write().write().unwrap().push(message.clone());
    push_transcript_message(messages, messages_revision, prompt_history, message);
}

/// Upsert a `transient:*` notice in the scrollable transcript (subtle grey ephemeral styling).
pub(crate) fn upsert_ephemeral_transcript_notice(
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
    key: &str,
    text: impl Into<String>,
) {
    let text = text.into();
    messages.set({
        let mut list = messages.read().clone();
        if let Some(row) = list.iter_mut().find(|m| m.startup_key.as_deref() == Some(key)) {
            row.content = text;
        } else {
            list.push(TranscriptMessage::startup_status(key, text, TranscriptStyle::Meta));
        }
        list
    });
    messages_revision.set(messages_revision.get().wrapping_add(1));
}

/// Remove a `transient:*` notice from the transcript when its TTL elapses (or it is replaced).
pub(crate) fn clear_ephemeral_transcript_notice(
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
    key: &str,
) {
    let before = messages.read().len();
    messages.set({
        let mut list = messages.read().clone();
        list.retain(|m| m.startup_key.as_deref() != Some(key));
        list
    });
    if messages.read().len() != before {
        messages_revision.set(messages_revision.get().wrapping_add(1));
    }
}

/// Show a timed transcript notice and schedule its auto-clear.
pub(crate) fn publish_ephemeral_transcript_notice(
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
    expires: &mut Ref<HashMap<&'static str, Instant>>,
    key: &'static str,
    text: impl Into<String>,
) {
    upsert_ephemeral_transcript_notice(messages, messages_revision, key, text);
    expires.write().insert(key, Instant::now() + AGENT_MODE_NOTICE_TTL);
}

/// Drop any transcript notices whose wall-clock TTL has elapsed.
pub(crate) fn poll_ephemeral_transcript_notices(
    messages: &mut State<Vec<TranscriptMessage>>,
    messages_revision: &mut State<u64>,
    expires: &mut Ref<HashMap<&'static str, Instant>>,
) {
    let now = Instant::now();
    let expired: Vec<&'static str> = expires
        .read()
        .iter()
        .filter(|(_, until)| now >= **until)
        .map(|(key, _)| *key)
        .collect();
    if expired.is_empty() {
        return;
    }
    for key in expired {
        clear_ephemeral_transcript_notice(messages, messages_revision, key);
        expires.write().remove(key);
    }
}

pub(crate) fn publish_transcript_now(
    messages_revision: &mut State<u64>,
    transcript_pending: &mut Ref<bool>,
    last_transcript_publish: &mut Ref<Instant>,
) {
    messages_revision.set(messages_revision.get().wrapping_add(1));
    transcript_pending.set(false);
    last_transcript_publish.set(Instant::now());
}

/// Adaptive publish interval: slower under large event bursts to keep UI input responsive.
pub(crate) fn transcript_publish_interval_ms(bootstrap_active: bool, event_burst: usize) -> u64 {
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
pub(crate) async fn apply_bootstrap_ui_event(
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
    prompt_history: &mut Ref<Vec<String>>,
    thinking_level: &mut State<ThinkingLevel>,
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
            thinking_level.set(restored_thinking_level_for_session(&bootstrap.session).await);
            {
                let mut msgs = messages.write();
                // Prepend persisted chat history so the transcript shows previous turns on resume.
                if !bootstrap.history_messages.is_empty() {
                    // Keep only the startup status lines, insert history before them.
                    let startup_lines: Vec<_> = msgs.iter().filter(|m| m.startup_key.is_some()).cloned().collect();
                    msgs.clear();
                    msgs.extend(bootstrap.history_messages.iter().cloned());
                    msgs.extend(startup_lines);
                    seed_history_from_transcript(&mut prompt_history.write(), &bootstrap.history_messages);
                }
                let provider = bootstrap.session.model_provider();
                let model = bootstrap.session.model_id();
                mark_agent_startup_ready(
                    &mut msgs,
                    (!provider.trim().is_empty()).then_some(provider.as_str()),
                    (!model.trim().is_empty()).then_some(model.as_str()),
                );
            }
            bootstrap_phase.set(BootstrapPhase::AgentReady);
            activity_label.set(bootstrap_activity_label(BootstrapPhase::AgentReady, None));
            // Unblock the prompt immediately — MCP tools load in the background.
            busy.set(false);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_thinking_level_converts_agent_value() {
        assert_eq!(
            thinking_level_from_agent(elph_agent::AgentThinkingLevel::High),
            ThinkingLevel::High
        );
    }

    #[test]
    fn worker_inbound_label_hides_intercom_wrapper() {
        let prompt = format!(
            "{} (`calm-fox`)\n\
             in this shared project. Answer it as part of your normal turn — you may use\n\
             tools. Reply with the `worker_reply` tool so the peer receives your answer.\n\
             If the message needs no answer, send a short acknowledgement.</intercom>\n\n\
             Please check the auth service",
            crate::agent::WORKER_INBOUND_PROMPT_PREFIX
        );
        let label = worker_inbound_meta_label(&prompt);
        assert!(label.starts_with("Message from worker calm-fox"), "{label}");
        assert!(label.contains("Please check the auth service"), "{label}");
        assert!(!label.contains("<intercom>"), "{label}");
        assert!(!label.contains("worker_reply"), "{label}");
    }

    #[test]
    fn worker_inbound_label_falls_back_without_sender() {
        let label = worker_inbound_meta_label("<intercom>plain");
        assert!(label.starts_with("Message from worker…"), "{label}");
    }
}

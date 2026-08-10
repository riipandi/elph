//! Worker chat — threaded inter-worker messaging (Alt+M / `/worker`).
//!
//! State lives in [`WorkerChatState`]. It is decoupled from the shell internals:
//! the tick loop calls [`drain_worker_inbox_events`] for new inbox events, keys
//! mutate the state via pure helpers, and the render phase paints
//! [`WorkerChatOverlay`]. Sending goes through `CodingAgentSession`
//! (`tui_send_worker_message`), never through an agent turn — so a chat message
//! never interrupts the user's current task.

use std::collections::HashMap;

use elph_agent::WorkerMessage;
use elph_agent::workers::MessageKind;
use iocraft::prelude::*;

use crate::agent::AgentUiEvent;
use crate::tui::focus::ShellFocus;

/// Max messages loaded for the inbox summary.
pub const WORKER_CHAT_INBOX_LIMIT: u64 = 200;

/// One visible conversation row (peer + last preview).
#[derive(Debug, Clone)]
pub struct WorkerChatRow {
    pub peer_worker_id: String,
    pub name: String,
    pub last_preview: String,
    pub unread: usize,
}

/// The worker chat overlay state (owned by the shell as a `Ref<Option<Self>>`).
#[derive(Debug, Clone)]
pub struct WorkerChatState {
    /// Current view: `None` = picker (worker list), `Some((worker_id, name))` = thread.
    pub active: Option<(String, String)>,
    /// Live peer workers for the picker (worker_id → name), refreshed on open.
    pub peers: Vec<WorkerChatRow>,
    /// Selected row index in the picker.
    pub selected: usize,
    /// Imported messages involving this session, oldest first (used to derive
    /// conversations + thread previews).
    pub messages: Vec<WorkerMessage>,
    /// Compose draft for the active thread.
    pub compose: String,
    /// The parent msg_id the active thread's last message replies to (threading).
    pub thread_parent: Option<String>,
    /// Conversation revision — bumped whenever a message arrives/sends so the
    /// render phase re-paints without a State dependency.
    pub revision: u64,
    /// Msg ids already seen by this shell session (dedupe on re-open).
    pub seen: std::collections::HashSet<String>,
}

impl Default for WorkerChatState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerChatState {
    pub fn new() -> Self {
        Self {
            active: None,
            peers: Vec::new(),
            selected: 0,
            messages: Vec::new(),
            compose: String::new(),
            thread_parent: None,
            revision: 0,
            seen: std::collections::HashSet::new(),
        }
    }

    /// Rebuild the picker rows from the last messages + live peer names.
    pub fn rebuild_peers(&mut self, live: Vec<elph_agent::LiveWorker>) {
        let live_names: HashMap<String, String> = live
            .into_iter()
            .map(|w| (w.worker_id.clone(), w.name.clone()))
            .collect();
        let mut order: Vec<String> = Vec::new();
        let mut names: HashMap<String, String> = HashMap::new();
        for (id, name) in &live_names {
            if !order.contains(id) {
                order.push(id.clone());
            }
            names.insert(id.clone(), name.clone());
        }
        // Add conversation partners seen in history (even if offline now).
        // Outbound rows carry an empty `from_worker_id` (self) — never show self.
        for msg in &self.messages {
            let peer = msg.to_worker_id.clone().unwrap_or_else(|| "".to_string());
            for id in [msg.from_worker_id.clone(), peer] {
                let id = id.as_str();
                if !id.is_empty() && !order.iter().any(|o| o == id) {
                    order.push(id.to_string());
                }
                if !id.is_empty() && !names.contains_key(id) {
                    names.insert(id.to_string(), id.to_string());
                }
            }
        }
        let mut rows: Vec<WorkerChatRow> = Vec::new();
        for id in order {
            if id == "__self__" {
                continue;
            }
            let (last_preview, unread) = self.peer_preview(&id);
            rows.push(WorkerChatRow {
                peer_worker_id: id.clone(),
                name: names.get(&id).cloned().unwrap_or_else(|| id.clone()),
                last_preview,
                unread,
            });
        }
        // Sort: unread first, then name.
        rows.sort_by(|a, b| {
            b.unread
                .cmp(&a.unread)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.peers = rows;
        self.selected = self.selected.min(self.peers.len().saturating_sub(1));
    }

    /// Latest preview + unread count for a peer conversation.
    fn peer_preview(&self, peer_worker_id: &str) -> (String, usize) {
        let mut unread = 0usize;
        let mut last: Option<&WorkerMessage> = None;
        for msg in &self.messages {
            let involves_peer =
                msg.from_worker_id == peer_worker_id || msg.to_worker_id.as_deref() == Some(peer_worker_id);
            if !involves_peer {
                continue;
            }
            let inbound_unseen = !self.seen.contains(&msg.id)
                && (msg.from_worker_id == peer_worker_id)
                && matches!(msg.kind, MessageKind::Prompt | MessageKind::Notify);
            if inbound_unseen {
                unread += 1;
            }
            last = Some(msg);
        }
        let preview = last
            .map(|m| crate::agent::extract_worker_payload_text(&m.payload))
            .unwrap_or_default();
        (preview, unread)
    }

    /// Messages displayed in the active thread (oldest first, capped).
    pub fn thread_messages(&self) -> Vec<&WorkerMessage> {
        let Some((peer_worker_id, _)) = &self.active else {
            return Vec::new();
        };
        self.messages
            .iter()
            .filter(|m| {
                m.from_worker_id == *peer_worker_id || m.to_worker_id.as_deref() == Some(peer_worker_id.as_str())
            })
            .collect()
    }

    /// Refresh the thread parent (last message in the active thread, if any).
    pub fn refresh_thread_parent(&mut self) {
        let msgs = self.thread_messages();
        self.thread_parent = msgs.last().map(|m| m.id.clone());
    }

    /// Mark everything in the active thread as seen (badge cleanup).
    pub fn mark_thread_seen(&mut self) {
        let Some((peer_worker_id, _)) = self.active.clone() else {
            return;
        };
        for msg in &self.messages {
            if (msg.from_worker_id == peer_worker_id || msg.to_worker_id.as_deref() == Some(peer_worker_id.as_str()))
                && !self.seen.contains(&msg.id)
            {
                self.seen.insert(msg.id.clone());
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }

    /// True when the overlay should render (picker open or thread active).
    #[allow(dead_code)]
    pub fn is_open(&self) -> bool {
        self.active.is_some() || !self.peers.is_empty()
    }

    /// Total unread count across all conversations (badge).
    #[allow(dead_code)]
    pub fn total_unread(&self) -> usize {
        self.peers.iter().map(|p| p.unread).sum()
    }

    /// True when an inbound message arrived that has not been seen yet (badge pulse).
    #[allow(dead_code)]
    pub fn has_unseen_inbound(&self) -> bool {
        self.messages
            .iter()
            .any(|m| !self.seen.contains(&m.id) && matches!(m.kind, MessageKind::Prompt | MessageKind::Notify))
    }
}

/// Handle one `AgentUiEvent::WorkerInbox*` event (from the tick loop drain).
pub fn apply_worker_inbox_event(state: &mut WorkerChatState, event: &AgentUiEvent) {
    match event {
        AgentUiEvent::WorkerInboxReceived {
            msg_id,
            from_worker_id,
            text,
            created_at,
            ..
        } => {
            // Synthesize a WorkerMessage-shaped row so the thread renders without a DB roundtrip.
            let msg = WorkerMessage {
                id: msg_id.clone(),
                project_key: String::new(),
                from_worker_id: from_worker_id.clone(),
                from_session_id: String::new(),
                to_worker_id: None,
                to_session_id: String::new(),
                kind: MessageKind::Prompt,
                status: elph_agent::workers::MessageStatus::Delivered,
                conversation_id: None,
                parent_msg_id: None,
                hops: 0,
                payload: serde_json::json!({ "text": text }).to_string(),
                created_at: created_at.clone(),
                delivered_at: None,
                completed_at: None,
                error: None,
            };
            state.messages.push(msg);
            state.rebuild_peers(Vec::new());
            state.revision = state.revision.wrapping_add(1);
        }
        AgentUiEvent::WorkerInboxSent {
            msg_id,
            to_worker_id,
            text,
            created_at,
            ..
        } => {
            let msg = WorkerMessage {
                id: msg_id.clone(),
                project_key: String::new(),
                from_worker_id: String::new(),
                from_session_id: String::new(),
                to_worker_id: Some(to_worker_id.clone()),
                to_session_id: String::new(),
                kind: MessageKind::Prompt,
                status: elph_agent::workers::MessageStatus::Queued,
                conversation_id: None,
                parent_msg_id: None,
                hops: 0,
                payload: serde_json::json!({ "text": text }).to_string(),
                created_at: created_at.clone(),
                delivered_at: None,
                completed_at: None,
                error: None,
            };
            state.messages.push(msg);
            state.seen.insert(msg_id.clone());
            state.rebuild_peers(Vec::new());
            state.revision = state.revision.wrapping_add(1);
        }
        AgentUiEvent::WorkerInboxUpdated => {
            state.revision = state.revision.wrapping_add(1);
        }
        _ => {}
    }
}

/// Drain the pending worker inbox events from a batch of drained agent UI events.
pub fn drain_worker_inbox_events(state: &mut WorkerChatState, events: &[AgentUiEvent]) {
    for event in events {
        apply_worker_inbox_event(state, event);
    }
}

/// Open the worker chat overlay from a slash / key handler.
///
/// Stashes the prompt draft and sets `ShellFocus::StatusDialog`. `history` is the
/// oldest-first inbox; only the last [`WORKER_CHAT_INBOX_LIMIT`] messages are kept.
pub fn open_worker_chat_overlay(
    pending: &mut Ref<Option<WorkerChatState>>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    peers: Vec<elph_agent::LiveWorker>,
    history: Vec<WorkerMessage>,
) {
    let stashed = {
        let current = live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        draft.set(String::new());
        live_draft.set(String::new());
    }
    let mut state = WorkerChatState::new();
    if history.len() > WORKER_CHAT_INBOX_LIMIT as usize {
        let start = history.len() - WORKER_CHAT_INBOX_LIMIT as usize;
        state.messages = history[start..].to_vec();
    } else {
        state.messages = history;
    }
    state.rebuild_peers(peers);
    if let Some(text) = stashed {
        state.compose = text;
    }
    pending.set(Some(state));
    shell_focus.set(ShellFocus::StatusDialog);
}

/// Close the worker chat overlay. Restores the stashed prompt draft when
/// `restore_stash` is true (Esc path), clears it otherwise (send/done path).
pub fn close_worker_chat(
    pending: &mut Ref<Option<WorkerChatState>>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
    restore_stash: bool,
) {
    let stashed = pending.write().take().and_then(|s| {
        let t = s.compose.trim().to_string();
        (!t.is_empty()).then_some(t)
    });
    if restore_stash {
        if let Some(text) = stashed {
            draft.set(text.clone());
            live_draft.set(text);
        } else {
            draft.set(String::new());
            live_draft.set(String::new());
        }
    } else {
        draft.set(String::new());
        live_draft.set(String::new());
    }
    shell_focus.set(ShellFocus::Prompt);
}

/// Select a thread from the picker (enter on a row).
pub fn select_worker_thread(state: &mut WorkerChatState, index: usize) -> Option<(String, String)> {
    let row = state.peers.get(index).cloned()?;
    state.active = Some((row.peer_worker_id.clone(), row.name.clone()));
    state.compose.clear();
    state.thread_parent = None;
    state.refresh_thread_parent();
    state.mark_thread_seen();
    Some((row.peer_worker_id, row.name))
}

/// Go back to the picker from a thread.
pub fn back_to_worker_picker(state: &mut WorkerChatState) {
    state.active = None;
    state.compose.clear();
    state.selected = state.selected.min(state.peers.len().saturating_sub(1));
}

/// Compose draft helpers — the shell key handler calls these directly, mirroring
/// the OAuth/pending input patterns (single-line input, Enter sends).
pub fn worker_compose_push(state: &mut WorkerChatState, c: char) {
    if c.is_control() {
        return;
    }
    state.compose.push(c);
    state.revision = state.revision.wrapping_add(1);
}

pub fn worker_compose_backspace(state: &mut WorkerChatState) -> bool {
    if state.compose.is_empty() {
        return false;
    }
    state.compose.pop();
    state.revision = state.revision.wrapping_add(1);
    true
}

pub fn worker_compose_clear(state: &mut WorkerChatState) {
    state.compose.clear();
    state.revision = state.revision.wrapping_add(1);
}

// ── Overlay ───────────────────────────────────────────────────────────

/// Props for [`WorkerChatOverlay`]. All data comes from the pending state; the
/// component never talks to the harness — pure presentation.
#[derive(Props)]
pub struct WorkerChatOverlayProps {
    pub screen_width: u16,
    pub screen_height: u16,
    pub state: WorkerChatState,
    /// Set from the shell's worker_chat_selected to highlight the picker row.
    pub picked: usize,
    pub on_esc: HandlerMut<'static, ()>,
    pub on_close: HandlerMut<'static, ()>,
}

impl Default for WorkerChatOverlayProps {
    fn default() -> Self {
        Self {
            screen_width: 80,
            screen_height: 24,
            state: WorkerChatState::new(),
            picked: 0,
            on_esc: HandlerMut::default(),
            on_close: HandlerMut::default(),
        }
    }
}

/// Centered overlay: picker (worker list) or thread view.
#[component]
pub fn WorkerChatOverlay(props: &mut WorkerChatOverlayProps, _hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = elph_tui::components::UiTheme::default();
    let width = (props.screen_width as u32 * 7 / 10).clamp(44, 96) as u16;
    let width = width.min(props.screen_width.saturating_sub(2).max(20));
    let height = props.screen_height.saturating_sub(6).clamp(10, 30);

    let chrome = elph_tui::components::DialogChrome {
        width,
        min_content_height: height,
        ..elph_tui::components::DialogChrome::default()
    };
    let header = if let Some((_, name)) = &props.state.active {
        elph_tui::components::DialogHeader::title(format!("Worker chat · {name}"))
    } else {
        elph_tui::components::DialogHeader::title("Worker chat")
    };

    let on_esc = props.on_esc.take();
    let _on_close = props.on_close.take();
    let state = props.state.clone();
    let picked = props.picked;

    let body_width = chrome.inner_body_width().max(1);
    let body_height = height.saturating_sub(3).max(4);

    let body: AnyElement<'static> = if let Some((peer_worker_id, name)) = &state.active {
        let messages_rows: Vec<AnyElement<'static>> = state
            .thread_messages()
            .iter()
            .map(|msg| {
                let outbound = msg.from_worker_id != *peer_worker_id;
                let text = crate::agent::extract_worker_payload_text(&msg.payload);
                let mut wrapped = elph_tui::wrap_text_to_lines(&text, body_width as usize - 4);
                if wrapped.is_empty() {
                    wrapped.push(String::new());
                }
                let rows: Vec<AnyElement<'static>> = wrapped
                    .into_iter()
                    .map(|line| {
                        element! {
                            Text(
                                content: line,
                                color: if outbound { theme.text_primary } else { theme.text_muted },
                                wrap: TextWrap::NoWrap,
                            )
                        }
                        .into()
                    })
                    .collect();
                let marker = if outbound { "»" } else { "«" };
                element! {
                    View(width: body_width, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                        Text(
                            content: format!(
                                "{marker} {} · {}",
                                if outbound { "you" } else { name },
                                short_time(&msg.created_at)
                            ),
                            color: theme.text_hint,
                            wrap: TextWrap::NoWrap,
                        )
                        #(rows)
                    }
                }
                .into()
            })
            .collect();

        element! {
            View(width: body_width, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                #(if messages_rows.is_empty() {
                    Some(element! {
                        Text(content: "(no messages yet — type below to start)".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
                    })
                } else {
                    None
                })
                View(
                    width: body_width,
                    height: body_height.saturating_sub(if messages_rows.is_empty() { 1 } else { 0 }),
                    overflow: Overflow::Hidden,
                    flex_shrink: 0f32,
                ) {
                    ScrollView(
                        auto_scroll: true,
                        scrollbar: Some(true),
                        scrollbar_thumb_color: Some(theme.warning),
                        scrollbar_track_color: Some(theme.text_muted),
                        keyboard_scroll: Some(false),
                    ) {
                        #(messages_rows)
                    }
                }
                Text(
                    content: format!("» {}", state.compose),
                    color: theme.input_text_color(true),
                    wrap: TextWrap::NoWrap,
                )
            }
        }
        .into()
    } else {
        // Picker.
        let rows: Vec<AnyElement<'static>> = state
            .peers
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let selected = i == picked;
                let marker = if selected { "▶" } else { " " };
                let preview: String = row.last_preview.chars().take(70).collect();
                let unread = if row.unread > 0 {
                    format!("  ⮐ {} new", row.unread)
                } else {
                    String::new()
                };
                element! {
                    View(width: body_width, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                        Text(
                            content: format!(
                                "{marker} {:<24} {}{}",
                                truncate_chars(&row.name, 24),
                                preview,
                                unread
                            ),
                            color: if selected { theme.text_primary } else { theme.text_muted },
                            wrap: TextWrap::NoWrap,
                        )
                    }
                }
                .into()
            })
            .collect();

        element! {
            View(width: body_width, flex_direction: FlexDirection::Column, flex_shrink: 0f32) {
                #(if rows.is_empty() {
                    Some(element! {
                        Text(content: "No workers found. Start another Elph session in this project to chat.".to_string(), color: theme.text_muted, wrap: TextWrap::NoWrap)
                    })
                } else {
                    None
                })
                #(rows)
            }
        }
        .into()
    };

    element! {
        elph_tui::components::DialogShellOverlay(
            screen_width: props.screen_width,
            screen_height: props.screen_height,
            chrome: chrome,
            header: header,
            theme: Some(theme),
            on_esc: on_esc,
            on_copy: None,
        ) {
            #(body)
        }
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

fn short_time(ts: &str) -> String {
    // created_at ISO like 2026-08-10T20:00:05Z → 20:00
    ts.get(11..16).unwrap_or("").trim_end_matches('Z').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, from: &str, to: Option<&str>, kind: MessageKind, text: &str) -> WorkerMessage {
        WorkerMessage {
            id: id.into(),
            project_key: String::new(),
            from_worker_id: from.into(),
            from_session_id: String::new(),
            to_worker_id: to.map(str::to_string),
            to_session_id: String::new(),
            kind,
            status: elph_agent::workers::MessageStatus::Delivered,
            conversation_id: None,
            parent_msg_id: None,
            hops: 0,
            payload: serde_json::json!({ "text": text }).to_string(),
            created_at: "2026-08-10T20:00:00Z".into(),
            delivered_at: None,
            completed_at: None,
            error: None,
        }
    }

    #[test]
    fn unread_counts_only_inbound_prompts() {
        let mut s = WorkerChatState::new();
        s.messages.push(msg("a", "w1", None, MessageKind::Prompt, "hi"));
        s.messages.push(msg("b", "", Some("w1"), MessageKind::Prompt, "yo"));
        s.messages.push(msg("c", "w1", None, MessageKind::Notify, "ping"));
        s.rebuild_peers(Vec::new());
        let row = s.peers.iter().find(|r| r.peer_worker_id == "w1").expect("row");
        assert_eq!(row.unread, 2);
        assert_eq!(s.total_unread(), 2);
    }

    #[test]
    fn thread_messages_filtered_by_peer() {
        let mut s = WorkerChatState::new();
        s.messages.push(msg("a", "w1", None, MessageKind::Prompt, "hi"));
        s.messages.push(msg("b", "", Some("w1"), MessageKind::Prompt, "yo"));
        s.messages.push(msg("c", "w2", None, MessageKind::Prompt, "other"));
        s.active = Some(("w1".into(), "w1".into()));
        assert_eq!(s.thread_messages().len(), 2);
    }
}

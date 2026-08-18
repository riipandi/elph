//! Stateful coding session wrapping `AgentHarness`.

mod compaction;
mod wiring;

use crate::types::AgentMode;
use anyhow::Result;
use elph_agent::{AgentHarness, AgentHarnessErrorCode, FileSystem};
use elph_agent::{GoalRuntime, McpToolRegistry, PlanConfirmationChoice, TursoSessionStorage};
use elph_ai::{AssistantMessage, StopReason};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};

use parking_lot::RwLock;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use super::aside::WORKER_INBOUND_PROMPT_PREFIX;
use super::aside::extract_worker_payload_text;
use super::events::AgentUiEvent;
use super::events::RETRY_CONTINUE_PROMPT;
use super::model_registry::ModelSelection;
use super::resource_loader::LoadResourcesResult;
use super::resource_loader::load_resources;

use super::prompt::{CodingPromptOptions, agents_md_for_cwd, build_coding_system_prompt};
use super::session_manager::SessionManager;
use super::tool_policy::AgentModePolicy;
use super::tool_policy::{from_agent_thinking, to_agent_thinking};
use super::tools_catalog::reconcile_harness_tools;
use crate::platform::Paths;
use elph_agent::parse_command_args;
use std::path::Path;

/// System prompt for background session title generation (`crates/coding-agent/prompts/`).
const SESSION_TITLE_SYSTEM: &str = include_str!("../../../prompts/session_title_system.txt");
/// User prompt template; `{{conversation}}` is replaced with the naming excerpt.
const SESSION_TITLE_USER: &str = include_str!("../../../prompts/session_title_user.txt");
/// Maximum number of background auto-title attempts per session lifetime.
const SESSION_TITLE_MAX_ATTEMPTS: u32 = 3;

/// Constructor inputs for [`CodingAgentSession::new`] (avoids a long positional arg list).
pub struct CodingAgentSessionParams {
    pub harness: Arc<AgentHarness<TursoSessionStorage>>,
    pub session_manager: SessionManager,
    pub session_id: String,
    pub selection: ModelSelection,
    pub agent_mode: AgentMode,
    pub mode_state: Arc<Mutex<AgentMode>>,
    pub show_thinking: bool,
    pub goal_runtime: Arc<GoalRuntime>,
    pub mcp_registry: Option<Arc<McpToolRegistry>>,
    pub ui_tx: mpsc::UnboundedSender<AgentUiEvent>,
    /// Model for auto session titles (`provider/model_id` or `"inherit"`).
    pub title_model: String,
    /// User's preferred chat language for transcript responses (e.g. `"english"`, `"indonesian"`).
    /// Code, comments, and documentation remain in English regardless of this value.
    pub preferred_chat_language: String,
    /// Settings `models.compactionModel` (`inherit` or `provider/model_id`).
    pub compaction_model_ref: String,
    /// Whether `codegraph.enabled` is on — gates the `<codegraph>` prompt section.
    pub codegraph_enabled: bool,
    /// Whether `simplifiedTechnicalEnglish` is on — gates the `<response_style>` section.
    pub ste_enabled: bool,
    /// Multi-worker host lifecycle (lease heartbeat + registry); None if start failed.
    pub worker_runtime: Option<super::worker_runtime::WorkerRuntime>,
}

pub struct CodingAgentSession {
    harness: Arc<AgentHarness<TursoSessionStorage>>,
    session_manager: SessionManager,
    session_id: String,
    /// Live model selection (updated by [`Self::set_model_from_value`] for Ctrl+P / picker).
    /// `Arc` so subagent event forwarders can read the current model live (provider/model_id).
    pub(crate) selection: Arc<RwLock<ModelSelection>>,
    policy: Arc<Mutex<AgentModePolicy>>,
    mode_state: Arc<Mutex<AgentMode>>,
    ui_tx: mpsc::UnboundedSender<AgentUiEvent>,
    show_thinking: bool,
    goal_runtime: Arc<GoalRuntime>,
    mcp_registry: Arc<RwLock<Option<Arc<McpToolRegistry>>>>,
    /// Serializes harness turns so only one prompt/template/compact runs at a time.
    turn_gate: Arc<Mutex<()>>,
    /// Watermark of the last `session_turns` record surfaced via `RunCompleted`; starts at
    /// -1 so turn index 0 counts as new. System operations that spin the UI without a real
    /// agent turn (`/compact`, ...) don't advance it, so their `RunCompleted` carries no stats.
    last_reported_turn_index: Arc<Mutex<AtomicI64>>,
    /// Serializes agent-mode reconciliation (Tab rapid cycling).
    mode_gate: Arc<Mutex<()>>,
    /// Last successfully compiled system prompt for sync slash reads during a busy turn.
    system_prompt_cache: RwLock<Option<String>>,
    /// Settings `models.sessionTitleModel` (`inherit` or `provider/model_id`).
    title_model: String,
    /// User's preferred chat language for transcript responses.
    /// Code, comments, and documentation remain in English regardless of this value.
    preferred_chat_language: String,
    /// Settings `models.compactionModel` (`inherit` or `provider/model_id`).
    compaction_model_ref: String,
    /// Whether `codegraph.enabled` is on — gates the `<codegraph>` prompt section.
    codegraph_enabled: bool,
    /// Whether `simplifiedTechnicalEnglish` is on — gates the `<response_style>` section.
    ste_enabled: bool,
    /// Bounded retry counter for background auto-title generation
    /// (caps at [`SESSION_TITLE_MAX_ATTEMPTS`] per session instance).
    title_generation_attempts: Arc<AtomicU32>,
    /// Multi-worker coordination (session lease heartbeat + presence registry).
    pub(crate) worker_runtime: Option<super::worker_runtime::WorkerRuntime>,
    /// True while an intercom (inbound worker-message) answer turn is running —
    /// either directly via `run_prompt_turn` or drained from the follow-up queue.
    /// Wiring and session emit paths read this to keep peer-to-peer dialogue out
    /// of the user's transcript (deltas, tool calls, stats, errors); the worker
    /// chat overlay is the only surface. Shared `Arc` so the harness subscriber
    /// (wiring) sees the same state.
    pub(crate) intercom_turn_active: Arc<AtomicBool>,
}

impl CodingAgentSession {
    pub async fn new(params: CodingAgentSessionParams) -> Result<Self> {
        let CodingAgentSessionParams {
            harness,
            session_manager,
            session_id,
            selection,
            agent_mode,
            mode_state,
            show_thinking,
            goal_runtime,
            mcp_registry,
            ui_tx,
            title_model,
            preferred_chat_language,
            compaction_model_ref,
            codegraph_enabled,
            ste_enabled,
            worker_runtime,
        } = params;
        let mut policy = AgentModePolicy::new(agent_mode);
        let mcp_slot = Arc::new(RwLock::new(mcp_registry));
        if let Some(reg) = mcp_slot.read().clone() {
            policy = policy.with_mcp_registry(reg);
        }
        // Resumed sessions that already have a title should not re-run generation.
        let already_named = harness.session_name().await.is_some();
        let session = Self {
            harness: harness.clone(),
            session_manager,
            session_id,
            selection: Arc::new(RwLock::new(selection)),
            policy: Arc::new(Mutex::new(policy)),
            mode_state,
            ui_tx: ui_tx.clone(),
            show_thinking,
            goal_runtime,
            mcp_registry: mcp_slot,
            turn_gate: Arc::new(Mutex::new(())),
            last_reported_turn_index: Arc::new(Mutex::new(AtomicI64::new(-1))),
            mode_gate: Arc::new(Mutex::new(())),
            system_prompt_cache: RwLock::new(None),
            title_model,
            preferred_chat_language,
            compaction_model_ref,
            codegraph_enabled,
            ste_enabled,
            title_generation_attempts: Arc::new(AtomicU32::new(if already_named {
                SESSION_TITLE_MAX_ATTEMPTS
            } else {
                0
            })),
            worker_runtime,
            intercom_turn_active: Arc::new(AtomicBool::new(false)),
        };
        session.wire_harness(ui_tx).await?;
        session.apply_agent_mode(agent_mode).await?;
        Ok(session)
    }

    /// Sync read of the last compiled system prompt (for `/system-prompt` while busy).
    pub fn cached_system_prompt(&self) -> Option<String> {
        self.system_prompt_cache.read().clone()
    }

    /// Recompile and store the system prompt snapshot used by sync slash handlers.
    pub async fn refresh_system_prompt_cache(&self) -> Result<()> {
        let text = self.compiled_system_prompt().await?;
        *self.system_prompt_cache.write() = Some(text);
        Ok(())
    }

    pub fn mode_state(&self) -> Arc<Mutex<AgentMode>> {
        self.mode_state.clone()
    }

    /// Live peer worker count (includes self when registry is up). 0 if workers not started.
    pub fn worker_live_count(&self) -> usize {
        self.worker_runtime.as_ref().map(|w| w.live_count()).unwrap_or(0)
    }

    /// TUI badge count: live workers when ≥2 and `tuiShowPeers`, else 0 (hidden).
    pub fn worker_tui_badge_count(&self) -> usize {
        self.worker_runtime
            .as_ref()
            .map(|w| w.tui_peer_badge_count())
            .unwrap_or(0)
    }

    /// Display name assigned in the project worker registry, if any.
    pub fn worker_name(&self) -> Option<&str> {
        self.worker_runtime.as_ref().map(|w| w.name.as_str())
    }

    /// True while an intercom (inbound worker-message) answer turn is running —
    /// the agent is replying to / sending a response for a peer worker.
    pub fn is_intercom_turn_active(&self) -> bool {
        self.intercom_turn_active.load(Ordering::Relaxed)
    }

    /// Graceful multi-worker teardown (release lease, mark offline, stop heartbeat).
    /// Safe to call with only `&self` (TUI holds `Arc<CodingAgentSession>`).
    pub async fn shutdown_workers(&self) {
        if let Some(rt) = self.worker_runtime.as_ref() {
            rt.shutdown().await;
        } else if self.session_manager.lease_worker_id().is_some()
            && let Err(err) = self.session_manager.release_session_lease(&self.session_id).await
        {
            log::warn!("release session lease: {err:#}");
        }
    }

    /// Send a **threaded** chat message to a peer worker from the TUI worker chat.
    ///
    /// Never routes through the agent turn — the message goes straight to the
    /// peer's mailbox thread (`worker_reply`-style semantics). Returns the inserted
    /// message. Must have a live worker runtime.
    pub async fn tui_send_worker_message(
        &self,
        peer: &elph_agent::LiveWorker,
        text: &str,
        parent_msg_id: Option<&str>,
    ) -> Result<elph_agent::WorkerMessage> {
        let Some(rt) = self.worker_runtime.as_ref() else {
            anyhow::bail!("worker runtime not started");
        };
        let message = match parent_msg_id {
            Some(parent) => {
                rt.mailbox()
                    .send_reply(
                        &rt.project_key,
                        &rt.worker_id,
                        &self.session_id,
                        &peer.session_id,
                        Some(&peer.worker_id),
                        parent,
                        None,
                        text,
                    )
                    .await?
            }
            None => {
                rt.mailbox()
                    .send_prompt(
                        &rt.project_key,
                        &rt.worker_id,
                        &self.session_id,
                        &peer.session_id,
                        Some(&peer.worker_id),
                        text,
                        0,
                        None,
                        None,
                    )
                    .await?
            }
        };
        let _ = self.ui_event_sender().send(AgentUiEvent::WorkerInboxSent {
            msg_id: message.id.clone(),
            to_worker: peer.name.clone(),
            to_worker_id: peer.worker_id.clone(),
            text: text.to_string(),
            created_at: message.created_at.clone(),
        });
        Ok(message)
    }

    /// All messages involving this session, oldest first (TUI worker inbox).
    pub async fn tui_worker_inbox(&self, limit: u64) -> Result<Vec<elph_agent::WorkerMessage>> {
        let Some(rt) = self.worker_runtime.as_ref() else {
            return Ok(Vec::new());
        };
        rt.mailbox().list_inbox(&self.session_id, limit).await
    }

    /// Per-peer conversation with a given worker (either side), oldest first.
    pub async fn tui_worker_conversation(
        &self,
        peer_worker_id: &str,
        limit: u64,
    ) -> Result<Vec<elph_agent::WorkerMessage>> {
        let Some(rt) = self.worker_runtime.as_ref() else {
            return Ok(Vec::new());
        };
        rt.mailbox()
            .list_conversation(&self.session_id, peer_worker_id, limit)
            .await
    }

    /// Live peer workers for the worker chat picker (excludes self).
    pub async fn tui_worker_peers(&self) -> Result<Vec<elph_agent::LiveWorker>> {
        let Some(rt) = self.worker_runtime.as_ref() else {
            return Ok(Vec::new());
        };
        rt.registry()
            .list_live_peers(&rt.project_key, &rt.worker_id, rt.stale_secs())
            .await
    }

    /// Mark fire-and-forget notifies as read (badge cleanup).
    pub async fn tui_mark_worker_notify_read(&self) -> Result<()> {
        let Some(rt) = self.worker_runtime.as_ref() else {
            return Ok(());
        };
        rt.mailbox().mark_notify_read(&self.session_id).await
    }

    /// Start durable mailbox inbox poller (claim → route to UI / agent turn). Call once after `Arc::new`.
    pub fn start_worker_inbox_poller(self: &Arc<Self>) {
        let Some(rt) = self.worker_runtime.as_ref() else {
            return;
        };
        let mailbox = rt.mailbox();
        let session_id = self.session_id.clone();
        let poll_ms = rt.inbox_poll_ms();
        let stop = rt.stop_flag();
        let session = Arc::clone(self);
        let handle = tokio::spawn(async move {
            let interval = std::time::Duration::from_millis(poll_ms);
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                match mailbox.claim_next_inbound(&session_id).await {
                    Ok(Some(msg)) => {
                        if let Err(err) = session.deliver_worker_inbound(msg).await {
                            log::warn!("worker inbox deliver: {err:#}");
                        }
                    }
                    Ok(None) => {}
                    Err(err) => log::debug!("worker inbox poll: {err:#}"),
                }
                tokio::time::sleep(interval).await;
            }
        });
        rt.set_inbox_handle(handle);
    }

    /// Deliver one inbound worker message — **never interrupts the user's task**.
    ///
    /// The message lands in the worker chat inbox (TUI) and, **only when it is a
    /// new message (no `parent_msg_id`)**, becomes a real agent turn (full
    /// context, tools, reply via `worker_reply`). Threaded replies — the
    /// `worker_reply` / TUI chat answers a peer sends back — are delivered to the
    /// inbox only: they resolve the asker's pending ask through the mailbox
    /// (`get_response_for`), and never spawn a reply turn. Without this guard two
    /// idle workers replying to each other would ping-pong forever (each reply is
    /// itself a kind-`prompt` message that would trigger another answer turn).
    ///
    /// While the main agent is busy a new-message answer turn is **enqueued as a
    /// follow-up** so it still runs right after the user's task — the peer's
    /// `worker_ask` does not silently time out. Never takes `turn_gate` directly
    /// and never calls the steer queue, so a peer message can never steal or
    /// interrupt the current agent turn.
    async fn deliver_worker_inbound(&self, msg: elph_agent::WorkerMessage) -> Result<()> {
        // Resolve display name for the sender (registry may be gone → fall back to id).
        let from_worker = if let Some(rt) = self.worker_runtime.as_ref() {
            rt.registry()
                .name_for_worker_id(&msg.from_worker_id)
                .await
                .unwrap_or_else(|| msg.from_worker_id.clone())
        } else {
            msg.from_worker_id.clone()
        };
        let text = extract_worker_payload_text(&msg.payload);

        // Surface to the TUI inbox + badge.
        let _ = self.ui_event_sender().send(AgentUiEvent::WorkerInboxReceived {
            msg_id: msg.id.clone(),
            from_worker: from_worker.clone(),
            from_worker_id: msg.from_worker_id.clone(),
            text: text.clone(),
            created_at: msg.created_at.clone(),
        });

        // Threaded reply (the answer to a message we asked / sent): inbox only.
        // The asker's worker_get/worker_await already unblocks via the mailbox
        // parent lookup; spawning a turn here would start an endless ping-pong.
        if msg.parent_msg_id.is_some() {
            return Ok(());
        }

        // New message → answer with a real turn (full context + tools).
        // `run_prompt_turn` takes the turn gate and waits for the user's current
        // task to finish (never injected as a steer, never interrupting), and it
        // sets the intercom flag so this whole turn stays out of the transcript.
        let prompt = self.worker_inbound_prompt(&from_worker, &text, &msg.id);
        if let Err(err) = self.run_prompt_turn(prompt, None).await {
            log::warn!("worker answer turn failed: {err:#}");
            self.reply_worker_answer_failed(&msg, &err.to_string()).await;
        }
        Ok(())
    }

    /// Build the turn prompt for answering an inbound worker message.
    ///
    /// The `WORKER_INBOUND_PROMPT_PREFIX` marker lets the TUI render this as a slim
    /// meta line instead of a user prompt card (see `worker_inbound_meta_label`).
    /// `msg_id` is pinned so `worker_reply` targets the exact ask — no ambiguity,
    /// no "N pending inbound messages" error when several asks are open.
    fn worker_inbound_prompt(&self, from_worker: &str, text: &str, msg_id: &str) -> String {
        format!(
            "{WORKER_INBOUND_PROMPT_PREFIX} (`{from_worker}`)\n\
             in this shared project. Answer it as part of your normal turn — you may use\n\
             tools. Reply with the `worker_reply` tool so the peer receives your answer.\n\
             Pass `in_reply_to` = {msg_id} (the message you are answering).\n\
             If the message needs no answer, send a short acknowledgement.</intercom>\n\n\
             {text}"
        )
    }

    /// Queue a normal agent turn that answers an inbound worker message.
    ///
    /// Runs `run_prompt_turn` (which takes `turn_gate`, so it serializes behind
    /// the user's current turn if racing) with a short intercom wrapper
    /// that tells the agent this is a peer message and to reply with
    /// `worker_reply`.
    async fn reply_worker_answer_failed(&self, msg: &elph_agent::WorkerMessage, error: &str) {
        log::warn!("worker answer failed for {}: {error}", msg.id);
        // The peer's ask should not hang until timeout: surface the failure as an
        // explicit error reply (kind `response`) so their worker_await unblocks.
        if let Some(rt) = self.worker_runtime.as_ref() {
            let _ = rt
                .mailbox()
                .send_response(
                    &msg.project_key,
                    rt.worker_id.as_str(),
                    &self.session_id,
                    &msg.from_session_id,
                    &msg.id,
                    "",
                    Some(error),
                )
                .await;
        }
    }

    /// Eagerly invalidate the system prompt cache synchronously so the next
    /// `/system-prompt` read (or fresh compile) reflects the current mode.
    ///
    /// Safe to call from the TUI input path while a turn is streaming.
    /// The cache is repopulated by [`apply_agent_mode`] when the mode-change
    /// background task completes, or on the next fresh compile via
    /// [`system_prompt_slash_message`].
    pub fn invalidate_system_prompt_cache(&self) {
        *self.system_prompt_cache.write() = None;
    }

    /// Try to set the agent mode synchronously using `try_lock`.
    ///
    /// Returns `true` on success. Falls back to `set_agent_mode` (async) when
    /// the lock is contended (unlikely — `mode_state` is held only briefly).
    ///
    /// Use this from the TUI key handler to eagerly update `mode_state` before
    /// spawning the full `set_agent_mode` background task, eliminating the race
    /// between mode change and the next prompt submission.
    pub fn try_set_mode_sync(&self, mode: AgentMode) -> bool {
        if let Ok(mut guard) = self.mode_state.try_lock() {
            *guard = mode;
            true
        } else {
            false
        }
    }

    /// Re-apply tool permissions after MCP hot-reload or tool set changes.
    pub async fn reconcile_tool_surface(&self) -> Result<()> {
        let mode = *self.mode_state.lock().await;
        self.apply_agent_mode(mode).await
    }

    pub fn mcp_registry(&self) -> Option<Arc<McpToolRegistry>> {
        self.mcp_registry.read().clone()
    }

    /// Late-bind MCP tools discovered after the TUI is visible.
    pub async fn attach_mcp_registry(&self, registry: Arc<McpToolRegistry>) -> Result<()> {
        let mcp_tools = registry.create_agent_tools().await;
        let mut kept: Vec<_> = self
            .harness
            .get_tools()
            .await
            .into_iter()
            .filter(|t| !t.name().starts_with("mcp_"))
            .collect();
        kept.extend(mcp_tools);
        self.harness
            .set_tools(kept, None)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        *self.mcp_registry.write() = Some(Arc::clone(&registry));
        self.policy.lock().await.set_mcp_registry(registry);
        let mode = *self.mode_state.lock().await;
        self.apply_agent_mode(mode).await
    }

    pub fn ui_event_sender(&self) -> mpsc::UnboundedSender<AgentUiEvent> {
        self.ui_tx.clone()
    }

    pub fn harness(&self) -> Arc<AgentHarness<TursoSessionStorage>> {
        self.harness.clone()
    }

    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }

    /// Every provider/model the live session catalog knows (`provider/model_id`, display name).
    pub fn list_acp_models(&self) -> Vec<(String, String)> {
        let selection = self.selection.read();
        let mut providers: Vec<_> = selection.models.get_providers();
        providers.sort_by(|a, b| a.id.cmp(&b.id));
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for provider in providers {
            let mut models = provider.get_models();
            models.sort_by(|a, b| a.id.cmp(&b.id));
            for model in models {
                let id = format!("{}/{}", provider.id, model.id);
                if seen.insert(id.clone()) {
                    out.push((id, format!("{}/{}", provider.id, model.name)));
                }
            }
        }
        out
    }

    pub fn model_display(&self) -> String {
        let selection = self.selection.read();
        format!("{} [{}/{}]", selection.display_name, selection.provider, selection.model_id)
    }

    pub fn model_provider(&self) -> String {
        self.selection.read().provider.clone()
    }

    pub fn model_id(&self) -> String {
        self.selection.read().model_id.clone()
    }

    /// Provider API id for the live model (e.g. `openai-responses`).
    pub fn model_api(&self) -> String {
        self.selection.read().model.api.clone()
    }

    /// Settings `models.sessionTitleModel` ref (`inherit` or `provider/model_id`).
    pub fn title_model(&self) -> String {
        self.title_model.clone()
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn context_window(&self) -> u32 {
        self.selection.read().model.context_window
    }

    pub fn supports_image_input(&self) -> bool {
        self.selection.read().model.input.iter().any(|cap| cap == "image")
    }

    pub fn goal_runtime(&self) -> Arc<GoalRuntime> {
        self.goal_runtime.clone()
    }

    /// Render the system prompt that would be sent on the next agent turn.
    pub async fn compiled_system_prompt(&self) -> Result<String> {
        let cwd_string = self.harness().env().cwd().to_string();
        let cwd = Path::new(&cwd_string);
        let resources = self.harness().get_resources().await;
        let tools = self.harness().get_active_tools().await;
        let tool_names: Vec<String> = tools.iter().map(|tool| tool.name().to_string()).collect();
        let agents_md = agents_md_for_cwd(cwd);
        let mode = *self.mode_state.lock().await;
        let worker_peers = if let Some(rt) = self.worker_runtime.as_ref() {
            let s = rt.peer_names_summary().await;
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        };
        let text = build_coding_system_prompt(
            cwd,
            &resources,
            &tool_names,
            agents_md.as_deref(),
            &CodingPromptOptions {
                mode,
                preferred_chat_language: self.preferred_chat_language.clone(),
                codegraph_enabled: self.codegraph_enabled,
                ste_enabled: self.ste_enabled,
                worker_name: self.worker_name().map(str::to_string),
                worker_peers,
            },
        )?;
        *self.system_prompt_cache.write() = Some(text.clone());
        Ok(text)
    }

    pub async fn set_agent_mode(&self, mode: AgentMode) -> Result<()> {
        let _guard = self.mode_gate.lock().await;
        *self.mode_state.lock().await = mode;
        self.policy.lock().await.set_mode(mode);
        // Wait for any in-flight turn before reconciling tools (avoids mid-turn mode races).
        let _turn_guard = self.turn_gate.lock().await;
        self.apply_agent_mode(mode).await
    }

    pub async fn set_thinking_level(&self, level: crate::types::ThinkingLevel) -> Result<()> {
        self.harness
            .set_thinking_level(to_agent_thinking(level))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    pub async fn submit_prompt(&self, text: String, steer: bool) -> Result<()> {
        self.submit_prompt_with(text, steer, None).await
    }

    pub async fn submit_prompt_with(
        &self,
        text: String,
        steer: bool,
        images: Option<Vec<elph_ai::ImageContent>>,
    ) -> Result<()> {
        if steer {
            // Mid-turn interjection: enqueue only — never wait_for_idle / RunCompleted.
            return self.queue_steer(text).await;
        }
        // Keep the session row's `updated_at` current so the resume list and
        // retention ordering reflect real activity even when the turn appends no
        // tree entries before the DB write path touches it.
        if let Err(err) = self.harness.touch_session_timestamp().await {
            log::debug!("touch session timestamp: {err:#}");
        }
        self.run_prompt_turn(text, images).await
    }

    /// Start a normal harness turn (blocks until idle, emits `RunCompleted`).
    async fn run_prompt_turn(&self, text: String, images: Option<Vec<elph_ai::ImageContent>>) -> Result<()> {
        let _guard = self.turn_gate.lock().await;
        // Intercom (worker-message) answer turns — identified by their prompt
        // prefix — set the shared flag for the whole turn: wiring suppresses
        // transcript events, and session emit paths (RunCompleted, retry status)
        // skip stats/notices for them. Cleared in both normal and error paths.
        let intercom = text.starts_with(WORKER_INBOUND_PROMPT_PREFIX);
        if intercom {
            self.intercom_turn_active.store(true, Ordering::Relaxed);
        }
        // Lazy MCP: discover any still-pending servers and hot-attach tools before the model runs.
        self.ensure_mcp_tools_ready().await;
        // Pre-prompt guard: when history already exceeds the configured threshold, compact
        // before sending so the request never runs into the hard context limit.
        self.maybe_auto_compact(Some(&text)).await;

        let started = Instant::now();
        let options = images.map(|images| elph_agent::AgentHarnessPromptOptions { images: Some(images) });
        let result = self.harness.prompt(text.clone(), options).await;
        match &result {
            Ok(message) => {
                if message.stop_reason == StopReason::Error {
                    // Turn ended in a provider/context error: recover (compact + one bounded
                    // retry) before finalizing the UI turn.
                    self.finish_ui_turn(started).await;
                    self.recover_errored_turn(message).await;
                } else {
                    // Compact *while the turn is still busy* so the loading indicator stays
                    // coherent: the agent visibly finalizes history (status row shows
                    // "Auto-compacting history…") instead of a frozen turn that emits
                    // post-hoc notices after the prompt box reappears.
                    self.maybe_auto_compact(None).await;
                    self.finish_ui_turn(started).await;
                }
                self.maybe_generate_session_title();
            }
            Err(err) if err.code == AgentHarnessErrorCode::Busy => {
                self.finish_ui_turn_rejected_busy(format!("Error: {err}")).await;
            }
            Err(err) => {
                let err_s = err.to_string();
                // Harness-level failure (e.g. stream cut) — one automatic retry when
                // transient. Keep the UI turn alive across the retry (no intermediate
                // RunCompleted) so the shell shows a spinner + "Retrying…" indicator.
                // The retry submits a Continue-style prompt instead of re-sending the
                // original text, so already-completed tool work is not duplicated.
                if elph_ai::retry::is_transient_error(&err_s) {
                    if !intercom {
                        let _ = self
                            .ui_tx
                            .send(AgentUiEvent::Status("Stream interrupted — retrying automatically…".to_string()));
                        let _ = self.ui_tx.send(AgentUiEvent::Retrying { attempt: 1 });
                    }
                    match self.harness.prompt(RETRY_CONTINUE_PROMPT, None).await {
                        Ok(msg) => {
                            self.finish_ui_turn(started).await;
                            if msg.stop_reason == StopReason::Error {
                                self.emit_retryable_error(msg.error_message.as_deref());
                            }
                            self.maybe_generate_session_title();
                            self.maybe_auto_compact(None).await;
                            if intercom {
                                self.intercom_turn_active.store(false, Ordering::Relaxed);
                            }
                            return Ok(());
                        }
                        Err(retry_err) => {
                            self.finish_ui_turn(started).await;
                            self.emit_retryable_error(Some(&retry_err.to_string()));
                            self.maybe_auto_compact(None).await;
                            if intercom {
                                self.intercom_turn_active.store(false, Ordering::Relaxed);
                            }
                            return Err(anyhow::anyhow!("{retry_err}"));
                        }
                    }
                }
                // Non-transient harness error. When the provider rejects the request with a
                // hard context-overflow error (rather than an assistant `stop_reason::Error`),
                // compact once and auto-resume the interrupted task with a Continue-style
                // prompt — the same recovery path used when a turn ends in `stop_reason::Error`.
                // Without this the agent would stop after compaction, looking frozen.
                self.finish_ui_turn(started).await;
                let recovered = self.recover_from_turn_error(&err_s).await;
                if recovered {
                    self.retry_after_compaction().await;
                } else {
                    self.emit_retryable_error(Some(&err_s));
                    self.maybe_auto_compact(None).await;
                }
                return if recovered {
                    if intercom {
                        self.intercom_turn_active.store(false, Ordering::Relaxed);
                    }
                    Ok(())
                } else {
                    if intercom {
                        self.intercom_turn_active.store(false, Ordering::Relaxed);
                    }
                    Err(anyhow::anyhow!("{err}"))
                };
            }
        }
        if intercom {
            self.intercom_turn_active.store(false, Ordering::Relaxed);
        }
        result.map(|_| ()).map_err(|err| anyhow::anyhow!("{err}"))
    }

    /// Ensure lazy MCP servers are discovered and tools attached to the harness.
    pub async fn ensure_mcp_tools_ready(&self) {
        let registry = {
            let guard = self.mcp_registry.read();
            guard.clone()
        };
        let Some(registry) = registry else {
            return;
        };
        let pending = registry.pending_server_count();
        if pending == 0 && registry.is_tools_discovered() {
            return;
        }
        let before = registry.tool_count();
        match tokio::time::timeout(std::time::Duration::from_secs(12), registry.discover_tools()).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                log::warn!("MCP on-demand discovery: {err:#}");
            }
            Err(_) => {
                log::warn!("MCP on-demand discovery timed out after 12s");
                let _ = self
                    .ui_tx
                    .send(AgentUiEvent::Status("MCP discovery timed out after 12s; continuing.".into()));
            }
        }
        let after = registry.tool_count();
        if after != before || pending > 0 {
            if let Err(err) = self.attach_mcp_registry(registry).await {
                log::warn!("MCP re-attach after discovery: {err:#}");
            } else if after > before {
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!(
                    "MCP: loaded {} tool(s) on demand",
                    after - before
                )));
            }
        }
    }

    fn emit_retryable_error(&self, error: Option<&str>) {
        // Intercom turn errors go to the worker chat, not the transcript.
        if self.intercom_turn_active.load(Ordering::Relaxed) {
            return;
        }
        let raw = error.unwrap_or("request failed");
        let display = crate::tui::api_error_display::format_user_facing_api_error(raw);
        let transient = elph_ai::retry::is_transient_error(raw);
        let line = if transient {
            format!("{display}\n\n\n{}", crate::tui::api_error_display::RETRY_HINT)
        } else {
            display
        };
        let _ = self.ui_tx.send(AgentUiEvent::Status(line));
        if transient {
            // Manual retry (Ctrl+R) resumes with a Continue-style recovery prompt rather
            // than re-sending the original text, so completed tool work is not duplicated.
            let _ = self
                .ui_tx
                .send(AgentUiEvent::RetryablePrompt(RETRY_CONTINUE_PROMPT.to_string()));
        }
    }

    /// After a turn that ended with a provider error, auto-recover when possible.
    ///
    /// 1. Transient stream/network errors → one automatic Continue-style retry.
    /// 2. Context-limit errors → compact then resume once.
    async fn recover_errored_turn(&self, message: &AssistantMessage) {
        // Intercom (worker-message) turns never auto-recover: retrying would run
        // a user-visible turn (no intercom prefix) and leak into the transcript.
        // The caller already replied with an explicit error through the mailbox.
        if self.intercom_turn_active.load(Ordering::Relaxed) {
            return;
        }
        let error_text = message.error_message.as_deref().unwrap_or_default();

        // Transient stream cutoffs / 5xx / etc. — retry without compaction first.
        if elph_ai::retry::is_transient_error(error_text) {
            if !self.intercom_turn_active.load(Ordering::Relaxed) {
                let _ = self
                    .ui_tx
                    .send(AgentUiEvent::Status("Stream interrupted — retrying automatically…".to_string()));
                let _ = self.ui_tx.send(AgentUiEvent::Retrying { attempt: 1 });
            }
            let retry_started = Instant::now();
            match self.harness.prompt(RETRY_CONTINUE_PROMPT, None).await {
                Ok(retry_message) => {
                    self.finish_ui_turn(retry_started).await;
                    if retry_message.stop_reason == StopReason::Error {
                        // Fall through to context recovery if still failing.
                        let retry_err = retry_message.error_message.as_deref().unwrap_or_default();
                        if self.recover_from_turn_error(retry_err).await {
                            self.retry_after_compaction().await;
                        } else {
                            self.emit_retryable_error(Some(retry_err));
                        }
                    }
                    return;
                }
                Err(err) => {
                    self.finish_ui_turn(retry_started).await;
                    log::warn!("auto-retry after stream error failed: {err}");
                    self.emit_retryable_error(Some(&err.to_string()));
                    return;
                }
            }
        }

        if !self.recover_from_turn_error(error_text).await {
            self.emit_retryable_error(Some(error_text));
            return;
        }
        self.retry_after_compaction().await;
    }

    async fn retry_after_compaction(&self) {
        let retry_started = Instant::now();
        if !self.retry_fits_after_compaction(RETRY_CONTINUE_PROMPT).await {
            let _ = self.ui_tx.send(AgentUiEvent::Status(
                "Context still exceeds limit after compaction — use /compact or a shorter prompt.".to_string(),
            ));
            self.finish_ui_turn(retry_started).await;
            return;
        }
        if !self.intercom_turn_active.load(Ordering::Relaxed) {
            let _ = self.ui_tx.send(AgentUiEvent::Retrying { attempt: 2 });
        }
        match self.harness.prompt(RETRY_CONTINUE_PROMPT, None).await {
            Ok(retry_message) => {
                self.finish_ui_turn(retry_started).await;
                if retry_message.stop_reason == StopReason::Error
                    && let Some(retry_error) = retry_message.error_message
                {
                    self.emit_retryable_error(Some(&retry_error));
                }
            }
            Err(err) => {
                self.finish_ui_turn(retry_started).await;
                log::warn!("auto-resume after compaction failed: {err}");
                self.emit_retryable_error(Some(&err.to_string()));
            }
        }
    }

    // maybe_auto_compact: see compaction.rs

    /// Enqueue a follow-up prompt (delivered after current agent work). Does not end the UI turn.
    ///
    /// If the harness is idle (UI busy flag desynced, bootstrap, race after turn end), starts a
    /// normal turn instead of failing with "Cannot follow up while idle".
    pub async fn queue_follow_up(&self, text: String) -> Result<()> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        match self.harness.follow_up(trimmed, None).await {
            Ok(()) => Ok(()),
            Err(err) if err.code == AgentHarnessErrorCode::InvalidState => {
                log::debug!("follow_up while idle — starting a normal turn");
                self.run_prompt_turn(trimmed.to_string(), None).await
            }
            Err(err) => Err(anyhow::anyhow!("{err}")),
        }
    }

    /// Enqueue a mid-turn steer / interjection. Does not end the UI turn.
    ///
    /// If the harness is idle, starts a normal turn instead of failing with "Cannot steer while idle".
    pub async fn queue_steer(&self, text: String) -> Result<()> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        match self.harness.steer(trimmed, None).await {
            Ok(()) => Ok(()),
            Err(err) if err.code == AgentHarnessErrorCode::InvalidState => {
                log::debug!("steer while idle — starting a normal turn");
                self.run_prompt_turn(trimmed.to_string(), None).await
            }
            Err(err) => Err(anyhow::anyhow!("{err}")),
        }
    }

    /// Promote the oldest follow-up onto the steer queue (one Ctrl+Enter while queues exist).
    pub async fn promote_next_follow_up_to_steer(&self) -> Result<Option<String>> {
        let message = self
            .harness
            .promote_follow_up_front_to_steer()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(message.map(|m| wiring::agent_message_preview(&m)))
    }

    /// Remove one queued item by kind and kind-local index. Returns the removed text.
    pub async fn remove_queued(
        &self,
        kind: super::events::QueuedPromptKind,
        kind_index: usize,
    ) -> Result<Option<String>> {
        use super::events::QueuedPromptKind;
        let message = match kind {
            QueuedPromptKind::FollowUp => self.harness.remove_follow_up_at(kind_index).await,
            QueuedPromptKind::Steer => self.harness.remove_steer_at(kind_index).await,
        }
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(message.map(|m| wiring::agent_message_preview(&m)))
    }

    /// Clear steer + follow-up queues (e.g. Ctrl+C). Emits QueueUpdate via harness.
    pub async fn clear_prompt_queues(&self) -> Result<()> {
        self.harness
            .clear_prompt_queues()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn abort(&self) -> Result<()> {
        self.harness
            .abort()
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn compact(&self) -> Result<()> {
        let _guard = self.turn_gate.lock().await;
        let started = Instant::now();
        let result = self
            .run_compact_with_notices(compaction::CompactSource::Manual, None, None, None)
            .await;
        self.finish_ui_turn(started).await;
        if let Err(err) = &result {
            self.notice_compact_failed(err);
        }
        result.map(|_| ())
    }

    /// Compact with runtime options (threshold, keep-recent, model, memory-flush).
    pub async fn compact_with_options(&self, options: crate::agent::slash_commands::CompactOptions) -> Result<()> {
        let _guard = self.turn_gate.lock().await;
        let started = Instant::now();

        // Build custom instructions from options
        let custom_instructions = if options.memory_flush {
            Some("First, run a memory flush to summarize important information from the conversation.".to_string())
        } else {
            None
        };

        // Resolve model override if provided
        let model_override = if let Some(model_ref) = &options.model {
            match compaction::resolve_settings_model_ref(model_ref, &self.selection.read().model) {
                Ok(m) => Some(m),
                Err(err) => {
                    let _ = self.ui_tx.send(AgentUiEvent::TranscriptNotice(format!(
                        "Invalid model reference '{model_ref}': {err}. Using session model."
                    )));
                    None
                }
            }
        } else {
            None
        };

        // Note: threshold_pct and keep_recent_tokens overrides would require extending
        // the harness API to accept per-operation settings. For now, only model and
        // memory-flush are fully supported.

        let result = self
            .run_compact_with_notices(
                compaction::CompactSource::Manual,
                custom_instructions.as_deref(),
                None,
                model_override.as_ref(),
            )
            .await;

        self.finish_ui_turn(started).await;
        if let Err(err) = &result {
            self.notice_compact_failed(err);
        }
        result.map(|_| ())
    }

    pub async fn reload_resources(&self, paths: &Paths, cwd: &Path) -> Result<LoadResourcesResult> {
        let env = self.harness.env();
        let loaded = load_resources(paths, cwd, env.as_ref()).await;
        self.harness
            .set_resources(loaded.resources.clone())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(loaded)
    }

    /// Replace live model selection (including the streaming [`elph_ai::Models`] Arc).
    pub(crate) fn replace_selection(&self, selection: ModelSelection) {
        *self.selection.write() = selection;
    }

    /// Inject a provider credential into the live [`elph_ai::Models`] store so the
    /// next stream uses it without restarting the session (after `/provider connect`).
    ///
    /// For GitHub Copilot OAuth, fills plan-available model ids when missing and
    /// re-filters the live catalog so Free/Student users only see supported models.
    pub async fn inject_provider_credential(&self, provider_id: &str, credential: elph_ai::Credential) {
        let models = Arc::clone(&self.selection.read().models);
        let credential = if provider_id == "github-copilot" {
            if let elph_ai::Credential::OAuth(mut oauth) = credential {
                let _ = elph_ai::ensure_copilot_available_model_ids(&mut oauth).await;
                elph_ai::Credential::OAuth(oauth)
            } else {
                credential
            }
        } else {
            credential
        };
        models.set_credential(provider_id, credential).await;
        log::info!("injected live credential for provider {provider_id}");
    }

    /// Reload one provider credential from `auth.json` into the live models store.
    pub async fn reload_provider_credential_from_disk(
        &self,
        auth_store_path: &std::path::Path,
        provider_id: &str,
    ) -> anyhow::Result<bool> {
        let models = Arc::clone(&self.selection.read().models);
        let loaded = super::model_registry::load_single_credential_from_auth_json(auth_store_path, provider_id).await?;
        if let Some(cred) = loaded {
            models.set_credential(provider_id, cred).await;
            log::info!("reloaded live credential for provider {provider_id} from auth.json");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn invoke_skill(&self, name: &str, args: &str) -> Result<()> {
        self.ensure_mcp_tools_ready().await;
        let _guard = self.turn_gate.lock().await;
        let started = Instant::now();
        let additional = (!args.trim().is_empty()).then(|| args.trim());
        let result = self.harness.skill(name, additional).await.map(|_| ());
        match &result {
            Ok(()) => {
                self.finish_ui_turn(started).await;
                self.maybe_generate_session_title();
            }
            Err(err) if err.code == AgentHarnessErrorCode::Busy => {
                self.finish_ui_turn_rejected_busy(format!("Skill error: {err}")).await;
            }
            Err(err) => {
                self.finish_ui_turn(started).await;
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Skill error: {err}")));
            }
        }
        result.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn prompt_from_template(&self, name: &str, args: &str) -> Result<()> {
        self.ensure_mcp_tools_ready().await;
        let _guard = self.turn_gate.lock().await;
        let started = Instant::now();
        let parsed = parse_command_args(args);
        let result = self.harness.prompt_from_template(name, &parsed).await.map(|_| ());
        match &result {
            Ok(()) => {
                self.finish_ui_turn(started).await;
                self.maybe_generate_session_title();
            }
            Err(err) if err.code == AgentHarnessErrorCode::Busy => {
                self.finish_ui_turn_rejected_busy(format!("Template error: {err}"))
                    .await;
            }
            Err(err) => {
                self.finish_ui_turn(started).await;
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Template error: {err}")));
            }
        }
        result.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn set_model_from_value(&self, value: &str) -> Result<String> {
        let _guard = self.turn_gate.lock().await;
        let model = super::overlays::resolve_model_from_value(value)?;
        let old_window = self.selection.read().model.context_window as u64;
        let new_window = model.context_window as u64;
        self.harness
            .set_model(model.clone())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // Keep live selection in sync so Ctrl+P cycle / chrome refresh see the new model.
        let display_name = model.name.clone();
        let provider = model.provider.clone();
        let model_id = model.id.clone();
        {
            let mut selection = self.selection.write();
            let models = Arc::clone(&selection.models);
            *selection = ModelSelection {
                provider: provider.clone(),
                model_id,
                model: model.clone(),
                models,
                display_name: display_name.clone(),
            };
        }
        // Clamp thinking to the new catalog map (live per-session state).
        let current_thinking = self.harness.get_thinking_level().await;
        let level = from_agent_thinking(current_thinking);
        let clamped = level.clamp_for_model(&model);
        if clamped != level {
            let _ = self.harness.set_thinking_level(to_agent_thinking(clamped)).await;
        }
        // If the new model has a smaller context window, compact until history fits.
        if let Err(err) = self.ensure_context_fits_new_model(old_window, new_window).await {
            log::warn!("model-switch fit compact: {err}");
        }
        Ok(format!("{display_name} [{provider}]"))
    }

    fn notice_compact_failed(&self, err: &anyhow::Error) {
        let _ = self
            .ui_tx
            .send(AgentUiEvent::TranscriptNotice(format!("Compaction failed: {err}")));
    }

    pub async fn navigate_tree_to(&self, entry_id: &str) -> Result<()> {
        self.navigate_tree_to_with_options(entry_id, false).await
    }

    /// Move the session leaf to `entry_id` (Pi `/tree` jump). Optional branch summary.
    pub async fn navigate_tree_to_with_options(&self, entry_id: &str, summarize: bool) -> Result<()> {
        use elph_agent::NavigateTreeOptions;
        self.harness
            .navigate_tree(
                entry_id,
                Some(NavigateTreeOptions {
                    summarize,
                    ..Default::default()
                }),
            )
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn branch_entries(&self) -> Result<Vec<elph_agent::SessionTreeEntry>> {
        self.harness
            .session_branch_entries()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// All session tree entries (full DAG), not just the active branch path.
    pub async fn session_tree_entries(&self) -> Result<Vec<elph_agent::SessionTreeEntry>> {
        Ok(self.harness.session_entries().await)
    }

    pub async fn leaf_id(&self) -> Result<Option<String>> {
        self.harness.session_leaf_id().await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Persist a full TUI transcript snapshot so `--resume` restores live card state
    /// (thinking, tools, durations, expand flags, edit_file diffs, …).
    ///
    /// **Deprecated:** This appends to the session tree which is append-only and never
    /// pruned — snapshots (7-8 MB each) accumulated to 600+ MB over a session. Use
    /// `save_transcript_snapshot_to_cache` instead, which overwrites the prior snapshot.
    pub async fn save_transcript_snapshot(&self, messages: &[crate::tui::transcript::TranscriptMessage]) -> Result<()> {
        use crate::tui::transcript::{TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE, build_snapshot_data};
        let data = build_snapshot_data(messages);
        self.harness
            .append_custom_entry(TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE, Some(data))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Persist the transcript snapshot to the TranscriptCache (overwrite semantics).
    ///
    /// This keeps only the latest snapshot per session, eliminating the unbounded
    /// growth from appending to the session tree. The `db_path` and `session_id`
    /// identify the per-project store DB (unified store.db).
    pub async fn save_transcript_snapshot_to_cache(
        &self,
        messages: &[crate::tui::transcript::TranscriptMessage],
        db_path: &std::path::Path,
        session_id: &str,
    ) -> Result<()> {
        use crate::tui::transcript::build_snapshot_data;
        let data = build_snapshot_data(messages);
        let json = serde_json::to_string(&data).map_err(|e| anyhow::anyhow!("{e}"))?;
        let cache = crate::tui::transcript::TranscriptCache::open(db_path, session_id).await?;
        cache.save_snapshot(&json).await?;
        Ok(())
    }

    pub async fn resolve_plan(&self, choice: PlanConfirmationChoice) -> Result<()> {
        self.harness
            .resolve_plan_confirmation(choice)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // Implementing a plan exits harness Plan mode — restore Build tool surface.
        if matches!(
            choice,
            PlanConfirmationChoice::Implement | PlanConfirmationChoice::ImplementFresh
        ) {
            *self.mode_state.lock().await = AgentMode::Build;
            self.policy.lock().await.set_mode(AgentMode::Build);
            self.apply_agent_mode(AgentMode::Build).await?;
        }
        Ok(())
    }

    /// Resolve plan confirmation with an optional saved plan file path.
    ///
    /// Stores the file path on the harness's pending plan so the implement prompt
    /// references the saved file instead of embedding the full plan text.
    pub async fn resolve_plan_with_file(
        &self,
        choice: PlanConfirmationChoice,
        plan_file: Option<String>,
    ) -> Result<()> {
        if let Some(ref path) = plan_file {
            self.harness
                .set_plan_file_path(path.clone())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
        self.resolve_plan(choice).await
    }

    /// Clear the pending plan on the harness (used when user chooses Revise
    /// so the agent can propose a revised plan).
    pub async fn clear_pending_plan(&self) -> Result<()> {
        self.harness
            .clear_pending_plan()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn apply_agent_mode(&self, mode: AgentMode) -> Result<()> {
        reconcile_harness_tools(&self.harness, mode, self.mcp_registry().as_deref()).await?;
        // Best-effort cache refresh so `/system-prompt` stays available without nesting
        // block_on on the UI thread during a busy stream.
        if let Err(err) = self.refresh_system_prompt_cache().await {
            log::debug!("system prompt cache refresh after mode change failed: {err:#}");
        }
        Ok(())
    }

    async fn finish_ui_turn(&self, started: Instant) {
        let _ = self.harness.wait_for_idle().await;
        if let Err(err) = self.refresh_system_prompt_cache().await {
            log::debug!("system prompt cache refresh after turn failed: {err:#}");
        }
        self.emit_run_completed(started).await;
    }

    /// Harness was busy when a follow-up turn was requested — surface status only so an
    /// in-flight turn keeps owning the shell busy indicator.
    async fn finish_ui_turn_rejected_busy(&self, status: String) {
        let _ = self.ui_tx.send(AgentUiEvent::Status(status));
    }

    async fn emit_run_completed(&self, started: Instant) {
        // Intercom (worker-message) turns stay silent in the transcript: no stats
        // card, no status line — the worker chat overlay is the only surface.
        if self.intercom_turn_active.load(Ordering::Relaxed) {
            let _ = self.ui_tx.send(AgentUiEvent::RunCompleted {
                elapsed_secs: started.elapsed().as_secs_f64(),
                usage: None,
                provider_id: None,
                model_id: None,
            });
            return;
        }
        // Only surface a stats card for real agent/chat-assistant turns, never for
        // system operations that only spin the UI (e.g. `/compact` with "History is
        // already up to date") — those do not produce a `session_turns` row, so
        // without this guard they would re-render the previous turn's record.
        let latest = {
            let harness = self.harness.clone();
            let sid = self.session_id.clone();
            tokio::task::spawn(async move { harness.current_turn_record(&sid).await })
                .await
                .ok()
                .flatten()
        };
        // Deterministic gate: the latest record must be newer than the last surfaced one.
        let is_new_turn = {
            let watermark = self.last_reported_turn_index.lock().await;
            latest
                .as_ref()
                .is_some_and(|r| r.turn_index > watermark.load(Ordering::SeqCst))
        };
        let Some(latest) = is_new_turn.then_some(latest).flatten() else {
            // Nothing new to report — degrade to an empty RunCompleted so the shell
            // still finalizes transcript state without emitting a stats card.
            let _ = self.ui_tx.send(AgentUiEvent::RunCompleted {
                elapsed_secs: started.elapsed().as_secs_f64(),
                usage: None,
                provider_id: None,
                model_id: None,
            });
            return;
        };
        self.last_reported_turn_index
            .lock()
            .await
            .store(latest.turn_index, Ordering::SeqCst);
        // Read the latest persisted turn record (harness writes usage right before idle)
        // so the shell can render turn-complete stats (tokens in/out/cached, model).
        // Missing store/turn degrades gracefully to `None` fields.
        let usage = Some(elph_agent::TurnUsage {
            input_tokens: latest.usage.input_tokens,
            output_tokens: latest.usage.output_tokens,
            cache_read_tokens: latest.usage.cache_read_tokens,
            cache_write_tokens: latest.usage.cache_write_tokens,
            total_tokens: latest.usage.total_tokens,
            cost: latest.usage.cost,
        });
        let _ = self.ui_tx.send(AgentUiEvent::RunCompleted {
            elapsed_secs: started.elapsed().as_secs_f64(),
            usage,
            provider_id: latest.provider_id,
            model_id: latest.model_id,
        });
    }

    /// After the first successful user turn, generate and persist a session title in the background.
    ///
    /// Silent on failure. Bounded retries: a failed or empty attempt does not
    /// permanently skip naming — later turns retry up to [`SESSION_TITLE_MAX_ATTEMPTS`].
    fn maybe_generate_session_title(&self) {
        let attempt = self.title_generation_attempts.fetch_add(1, Ordering::SeqCst);
        if attempt >= SESSION_TITLE_MAX_ATTEMPTS {
            return;
        }

        let harness = self.harness.clone();
        let models = {
            let selection = self.selection.read();
            Arc::clone(&selection.models)
        };
        let inherit_model = self.selection.read().model.clone();
        let title_model_setting = self.title_model.clone();
        let attempts = self.title_generation_attempts.clone();

        tokio::spawn(async move {
            match generate_and_store_session_title(harness, models, inherit_model, &title_model_setting).await {
                // Title stored — stop retrying.
                Ok(Some(_)) => attempts.store(SESSION_TITLE_MAX_ATTEMPTS, Ordering::SeqCst),
                // Nothing to name yet (or no fallback available) — retry on a later turn.
                Ok(None) => {}
                Err(err) => log::debug!("auto session title skipped: {err:#}"),
            }
        });
    }
}

/// Generate a session title in the background and persist it to the harness.
///
/// Returns `Ok(Some(title))` when a title was stored, `Ok(None)` when there is
/// nothing to name yet (caller may retry on a later turn).
async fn generate_and_store_session_title(
    harness: Arc<AgentHarness<TursoSessionStorage>>,
    models: Arc<elph_ai::Models>,
    inherit_model: elph_ai::Model,
    title_model_setting: &str,
) -> Result<Option<String>> {
    let branch = harness
        .session_branch_entries()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let context = elph_agent::build_session_context(&branch);
    let conversation = elph_agent::extract_conversation_for_naming(&context.messages);
    if conversation.trim().is_empty() {
        return Ok(None);
    }

    let model = resolve_title_model(title_model_setting, &inherit_model);
    let user_prompt = SESSION_TITLE_USER.replace("{{conversation}}", &conversation);
    // Naming model call first; fall back to the first user message when it fails
    // or returns a generic placeholder, so sessions always end up named.
    let title = elph_agent::generate_session_name_with_prompts(
        &context.messages,
        models.as_ref(),
        &model,
        SESSION_TITLE_SYSTEM,
        &user_prompt,
    )
    .await
    .or_else(|| fallback_session_title(&conversation));

    let Some(title) = title else {
        return Ok(None);
    };

    harness
        .set_session_name(title.clone())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Some(title))
}

/// Deterministic fallback title when the naming model call fails: the first
/// user message, sanitized and truncated to [`elph_agent::sanitize_session_name`].
fn fallback_session_title(conversation: &str) -> Option<String> {
    let first = conversation.split("\n\n").next()?.trim();
    let text = first.strip_prefix("User:").map(str::trim).unwrap_or(first);
    let title = elph_agent::sanitize_session_name(text);
    if title.is_empty() { None } else { Some(title) }
}

/// Resolve the session-title model ref, falling back to the session model when
/// the configured value is invalid or unknown (robustness over aborting naming).
fn resolve_title_model(setting: &str, inherit: &elph_ai::Model) -> elph_ai::Model {
    match compaction::resolve_settings_model_ref(setting, inherit) {
        Ok(model) => model,
        Err(err) => {
            log::debug!("session title model ref `{setting}` unresolved, using session model: {err}");
            inherit.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fallback_session_title, resolve_title_model};
    use elph_ai::get_builtin_model;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering};
    use tokio::sync::Mutex;

    struct TurnReporter {
        watermark: Arc<Mutex<AtomicI64>>,
    }

    impl TurnReporter {
        fn new() -> Self {
            Self {
                watermark: Arc::new(Mutex::new(AtomicI64::new(-1))),
            }
        }

        /// Mirrors the gate logic in [`CodingAgentSession::emit_run_completed`]: reports
        /// `turn_index` only when it's a new (higher) index than the last surfaced one.
        /// Returns the reported index, or `None` when the operation produced no new turn
        /// (e.g. `/compact` with "History is already up to date").
        async fn report(&self, latest_turn_index: Option<i64>) -> Option<i64> {
            let is_new = self.watermark.lock().await.load(Ordering::SeqCst);
            let latest = latest_turn_index.filter(|i| i > &is_new)?;
            self.watermark.lock().await.store(latest, Ordering::SeqCst);
            Some(latest)
        }
    }

    #[tokio::test]
    async fn stats_only_for_new_agent_turns() {
        let reporter = TurnReporter::new();
        // Real first agent turn (index 0) is reported.
        assert_eq!(reporter.report(Some(0)).await, Some(0));
        // System op with no new turn row (`/compact` no-op) is suppressed — even though
        // the previous turn's record is still the "latest".
        assert_eq!(reporter.report(Some(0)).await, None);
        assert_eq!(reporter.report(None).await, None);
        // A genuine follow-up turn is reported again.
        assert_eq!(reporter.report(Some(1)).await, Some(1));
    }

    #[test]
    fn title_model_inherit_uses_session_model() {
        let model = get_builtin_model("openai", "gpt-4o-mini").expect("builtin model");
        let resolved = resolve_title_model("inherit", &model);
        assert_eq!(resolved.id, model.id);
        assert_eq!(resolved.provider, model.provider);
    }

    #[test]
    fn title_model_empty_inherits() {
        let model = get_builtin_model("openai", "gpt-4o-mini").expect("builtin model");
        let resolved = resolve_title_model("  ", &model);
        assert_eq!(resolved.id, model.id);
    }

    #[test]
    fn title_model_resolves_explicit_ref() {
        let inherit = get_builtin_model("openai", "gpt-4o-mini").expect("builtin model");
        let explicit = get_builtin_model("anthropic", "claude-haiku-4-5").expect("builtin model");
        let resolved = resolve_title_model("anthropic/claude-haiku-4-5", &inherit);
        assert_eq!(resolved.id, explicit.id);
        assert_eq!(resolved.provider, explicit.provider);
    }

    #[test]
    fn title_model_invalid_ref_falls_back_to_session_model() {
        let model = get_builtin_model("openai", "gpt-4o-mini").expect("builtin model");
        let resolved = resolve_title_model("openai/does-not-exist-xyz", &model);
        assert_eq!(resolved.id, model.id);
    }

    #[test]
    fn fallback_title_uses_first_user_message() {
        let conversation = "User: Fix the login redirect for OAuth flows\n\n[...]\n\nUser: Ship it";
        assert_eq!(
            fallback_session_title(conversation).as_deref(),
            Some("Fix the login redirect for OAuth flows")
        );
        // Generic first messages produce no fallback (caller retries later).
        assert_eq!(fallback_session_title("User: hi"), None);
        assert_eq!(fallback_session_title(""), None);
    }
}

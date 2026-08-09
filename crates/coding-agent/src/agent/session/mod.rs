//! Stateful coding session wrapping `AgentHarness`.

mod compaction;
mod wiring;

use crate::types::AgentMode;
use anyhow::Result;
use elph_agent::{AgentHarness, AgentHarnessErrorCode, FileSystem};
use elph_agent::{GoalRuntime, McpToolRegistry, PlanConfirmationChoice, TursoSessionStorage};
use elph_ai::{AssistantMessage, StopReason};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::RwLock;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

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

/// System prompt for background session title generation (`crates/coding-agent/templates/agent/`).
const SESSION_TITLE_SYSTEM: &str = include_str!("../../../templates/agent/session_title_system.md");
/// User prompt template; `{{conversation}}` is replaced with the naming excerpt.
const SESSION_TITLE_USER: &str = include_str!("../../../templates/agent/session_title_user.md");
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
        if steer {
            // Mid-turn interjection: enqueue only — never wait_for_idle / RunCompleted.
            return self.queue_steer(text).await;
        }
        self.run_prompt_turn(text).await
    }

    /// Start a normal harness turn (blocks until idle, emits `RunCompleted`).
    async fn run_prompt_turn(&self, text: String) -> Result<()> {
        let _guard = self.turn_gate.lock().await;
        // Lazy MCP: discover any still-pending servers and hot-attach tools before the model runs.
        self.ensure_mcp_tools_ready().await;
        // Pre-prompt guard: when history already exceeds the configured threshold, compact
        // before sending so the request never runs into the hard context limit.
        self.maybe_auto_compact(Some(&text)).await;

        let started = Instant::now();
        let result = self.harness.prompt(text.clone(), None).await;
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
                    let _ = self
                        .ui_tx
                        .send(AgentUiEvent::Status("Stream interrupted — retrying automatically…".to_string()));
                    let _ = self.ui_tx.send(AgentUiEvent::Retrying { attempt: 1 });
                    match self.harness.prompt(RETRY_CONTINUE_PROMPT, None).await {
                        Ok(msg) => {
                            self.finish_ui_turn(started).await;
                            if msg.stop_reason == StopReason::Error {
                                self.emit_retryable_error(msg.error_message.as_deref());
                            }
                            self.maybe_generate_session_title();
                            self.maybe_auto_compact(None).await;
                            return Ok(());
                        }
                        Err(retry_err) => {
                            self.finish_ui_turn(started).await;
                            self.emit_retryable_error(Some(&retry_err.to_string()));
                            self.maybe_auto_compact(None).await;
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
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("{err}"))
                };
            }
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
        if let Err(err) = registry.discover_tools().await {
            log::warn!("MCP on-demand discovery: {err:#}");
            // Still re-attach whatever was discovered so far.
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
        let error_text = message.error_message.as_deref().unwrap_or_default();

        // Transient stream cutoffs / 5xx / etc. — retry without compaction first.
        if elph_ai::retry::is_transient_error(error_text) {
            let _ = self
                .ui_tx
                .send(AgentUiEvent::Status("Stream interrupted — retrying automatically…".to_string()));
            let _ = self.ui_tx.send(AgentUiEvent::Retrying { attempt: 1 });
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
        let _ = self.ui_tx.send(AgentUiEvent::Retrying { attempt: 2 });
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
                self.run_prompt_turn(trimmed.to_string()).await
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
                self.run_prompt_turn(trimmed.to_string()).await
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

    pub async fn invoke_skill(&self, name: &str, args: &str) -> Result<()> {
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
        self.harness
            .navigate_tree(entry_id, None)
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
        let _ = self.ui_tx.send(AgentUiEvent::RunCompleted {
            elapsed_secs: started.elapsed().as_secs_f64(),
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

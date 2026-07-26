//! Stateful coding session wrapping `AgentHarness`.

mod wiring;

use crate::types::AgentMode;
use anyhow::Result;
use elph_agent::{AgentHarness, AgentHarnessErrorCode, FileSystem};
use elph_agent::{GoalRuntime, McpToolRegistry, PlanConfirmationChoice, SessionDirStorage};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::RwLock;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use super::events::AgentUiEvent;
use super::model_registry::ModelSelection;
use super::resource_loader::LoadResourcesResult;
use super::resource_loader::load_resources;

use super::prompt::{agents_md_for_cwd, build_coding_system_prompt};
use super::session_manager::SessionManager;
use super::tool_policy::AgentModePolicy;
use super::tool_policy::to_agent_thinking;
use super::tools_catalog::reconcile_harness_tools;
use crate::platform::Paths;
use elph_agent::parse_command_args;
use std::path::Path;

/// System prompt for background session title generation (`elph/templates/agent/`).
const SESSION_TITLE_SYSTEM: &str = include_str!("../../../templates/agent/session_title_system.md");
/// User prompt template; `{{conversation}}` is replaced with the naming excerpt.
const SESSION_TITLE_USER: &str = include_str!("../../../templates/agent/session_title_user.md");

/// Constructor inputs for [`CodingAgentSession::new`] (avoids a long positional arg list).
pub struct CodingAgentSessionParams {
    pub harness: Arc<AgentHarness<SessionDirStorage>>,
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
}

pub struct CodingAgentSession {
    harness: Arc<AgentHarness<SessionDirStorage>>,
    session_manager: SessionManager,
    session_id: String,
    /// Live model selection (updated by [`Self::set_model_from_value`] for Ctrl+P / picker).
    selection: RwLock<ModelSelection>,
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
    /// Settings `session.titleModel` (`inherit` or `provider/model_id`).
    title_model: String,
    /// Ensures at most one background auto-title attempt per session instance.
    title_generation_started: AtomicBool,
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
            selection: RwLock::new(selection),
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
            title_generation_started: AtomicBool::new(already_named),
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
        let mcp_tools = registry.create_agent_tools();
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

    pub fn harness(&self) -> Arc<AgentHarness<SessionDirStorage>> {
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
        let text = build_coding_system_prompt(cwd, &resources, &tool_names, agents_md.as_deref(), mode)?;
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
        let started = Instant::now();
        let result = self.harness.prompt(text, None).await.map(|_| ());
        match &result {
            Ok(()) => {
                self.finish_ui_turn(started).await;
                self.maybe_generate_session_title();
            }
            Err(err) if err.code == AgentHarnessErrorCode::Busy => {
                self.finish_ui_turn_rejected_busy(format!("Error: {err}")).await;
            }
            Err(err) => {
                self.finish_ui_turn(started).await;
                let text = crate::tui::api_error_display::format_user_facing_api_error(&err.to_string());
                let _ = self.ui_tx.send(AgentUiEvent::Status(text));
            }
        }
        result.map_err(|err| anyhow::anyhow!("{err}"))
    }

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
        let result = self.harness.compact(None).await;
        self.finish_ui_turn(started).await;
        match &result {
            Ok(compact_result) if compact_result.is_noop() => {
                let _ = self
                    .ui_tx
                    .send(AgentUiEvent::Status("History is already up to date.".into()));
            }
            Ok(_) => {
                let _ = self.ui_tx.send(AgentUiEvent::Status("History compacted.".into()));
            }
            Err(err) => {
                let _ = self.ui_tx.send(AgentUiEvent::Status(format!("Compact failed: {err}")));
            }
        }
        result.map(|_| ()).map_err(|e| anyhow::anyhow!("{e}"))
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
        let model = super::overlays::resolve_model_from_value(value)?;
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
                model,
                models,
                display_name: display_name.clone(),
            };
        }
        Ok(format!("{display_name} [{provider}]"))
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
    pub async fn save_transcript_snapshot(&self, messages: &[crate::tui::transcript::TranscriptMessage]) -> Result<()> {
        use crate::tui::transcript::{TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE, build_snapshot_data};
        let data = build_snapshot_data(messages);
        self.harness
            .append_custom_entry(TRANSCRIPT_SNAPSHOT_CUSTOM_TYPE, Some(data))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
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
    /// Silent on failure; at most one attempt per session instance (skipped when already named).
    fn maybe_generate_session_title(&self) {
        if self
            .title_generation_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let harness = self.harness.clone();
        let models = {
            let selection = self.selection.read();
            Arc::clone(&selection.models)
        };
        let inherit_model = self.selection.read().model.clone();
        let title_model_setting = self.title_model.clone();

        tokio::spawn(async move {
            if let Err(err) =
                generate_and_store_session_title(harness, models, inherit_model, &title_model_setting).await
            {
                log::debug!("auto session title skipped: {err:#}");
            }
        });
    }
}

async fn generate_and_store_session_title(
    harness: Arc<AgentHarness<SessionDirStorage>>,
    models: Arc<elph_ai::Models>,
    inherit_model: elph_ai::Model,
    title_model_setting: &str,
) -> Result<()> {
    if harness.session_name().await.is_some() {
        return Ok(());
    }

    let branch = harness
        .session_branch_entries()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let context = elph_agent::build_session_context(&branch);
    let conversation = elph_agent::extract_conversation_for_naming(&context.messages);
    if conversation.trim().is_empty() {
        return Ok(());
    }

    let model = resolve_title_model(title_model_setting, &inherit_model)?;
    let user_prompt = SESSION_TITLE_USER.replace("{{conversation}}", &conversation);
    let Some(title) = elph_agent::generate_session_name_with_prompts(
        &context.messages,
        models.as_ref(),
        &model,
        SESSION_TITLE_SYSTEM,
        &user_prompt,
    )
    .await
    else {
        return Ok(());
    };

    harness
        .set_session_name(title)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

fn resolve_title_model(setting: &str, inherit: &elph_ai::Model) -> Result<elph_ai::Model> {
    let trimmed = setting.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
        return Ok(inherit.clone());
    }
    super::overlays::resolve_model_from_value(trimmed)
}

#[cfg(test)]
mod tests {
    use super::resolve_title_model;
    use elph_ai::get_builtin_model;

    #[test]
    fn title_model_inherit_uses_session_model() {
        let model = get_builtin_model("openai", "gpt-4o-mini").expect("builtin model");
        let resolved = resolve_title_model("inherit", &model).expect("resolve");
        assert_eq!(resolved.id, model.id);
        assert_eq!(resolved.provider, model.provider);
    }

    #[test]
    fn title_model_empty_inherits() {
        let model = get_builtin_model("openai", "gpt-4o-mini").expect("builtin model");
        let resolved = resolve_title_model("  ", &model).expect("resolve");
        assert_eq!(resolved.id, model.id);
    }
}

//! Session-backed subagent harness.

use std::sync::Arc;

use elph_ai::Model;

use super::control::AgentControl;
use super::registry::AgentRegistry;
use super::types::SubagentBootstrap;
use super::types::SubagentInfo;
use super::types::SubagentLimits;
use super::types::SubagentOutput;
use super::types::SubagentStatus;
use super::types::persist;
use crate::agent::harness::{AgentHarness, AgentHarnessError, AgentHarnessOptions, SystemPrompt};
use crate::runtime::local_env::LocalExecutionEnv;
use crate::session::{TursoSessionRepo, TursoSessionRepoCreateOptions, TursoSessionStorage};
use crate::types::{AgentTool, QueueMode};

pub struct SubagentHarness {
    harness: Arc<AgentHarness<TursoSessionStorage>>,
    info: SubagentInfo,
    /// Persistent artifact dir for this subagent (when configured).
    output_dir: Option<std::path::PathBuf>,
    /// Final assistant text of the last completed turn.
    last_output: tokio::sync::Mutex<String>,
    /// Completed assistant turns (initial spawn + follow-ups).
    turns: std::sync::atomic::AtomicU32,
    /// In-flight turn tracking so waiters never race a not-yet-started turn.
    inflight: std::sync::atomic::AtomicUsize,
    turn_notify: tokio::sync::Notify,
}

impl SubagentHarness {
    pub fn info(&self) -> &SubagentInfo {
        &self.info
    }

    pub fn harness(&self) -> &AgentHarness<TursoSessionStorage> {
        &self.harness
    }

    /// Model the subagent will use for its turns (inherited from the parent's
    /// active model at spawn time).
    pub async fn model(&self) -> elph_ai::Model {
        self.harness.get_model().await
    }

    /// Last completed turn output (final assistant text, trimmed).
    pub async fn last_output(&self) -> String {
        self.last_output.lock().await.trim().to_string()
    }

    pub async fn output(&self) -> SubagentOutput {
        let text = self.last_output().await;
        SubagentOutput {
            text,
            output_path: self
                .output_dir
                .as_ref()
                .map(|dir| dir.join(persist::OUTPUT_MD).to_string_lossy().to_string()),
            finished_at_ms: Some(now_ms()),
            turns: self.turns.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Mark that a turn is being dispatched (synchronous, so waiters observe it).
    pub fn turn_started(&self) {
        self.inflight.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Mark the currently dispatched turn as finished and wake waiters.
    pub fn turn_finished(&self) {
        self.inflight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        self.turn_notify.notify_waiters();
    }

    /// RAII guard that releases the in-flight slot on drop. Guarantees waiters
    /// are never stuck even if the background turn task panics or is dropped.
    pub fn turn_guard(self: &Arc<Self>) -> TurnGuard {
        self.turn_started();
        TurnGuard {
            harness: Arc::clone(self),
        }
    }

    /// Block until no turn is in flight. Handles the race where [`wait_agent`]
    /// runs before the spawned turn task reaches the harness run loop.
    pub async fn wait_for_turn(&self) {
        loop {
            if self.inflight.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                return;
            }
            self.turn_notify.notified().await;
        }
    }

    /// Subscribe a forwarder to this subagent's harness events. Must be called
    /// once after spawn — repeated calls accumulate duplicate subscribers.
    pub async fn forward_events(&self, forwarder: crate::agent::subagent::SubagentEventForwarder) {
        let info = self.info().clone();
        self.harness
            .subscribe_agent_events(Arc::new(move |event| {
                forwarder(event, &info);
            }))
            .await;
    }

    pub async fn followup(&self, message: String) -> Result<(), String> {
        let result: Result<elph_ai::AssistantMessage, String> =
            self.harness.prompt(message, None).await.map_err(|e| e.to_string());
        self.record_turn(result.as_ref().ok()).await;
        self.harness.wait_for_idle().await.map_err(|e| e.to_string())?;
        result.map(|_| ())
    }

    pub async fn wait_idle(&self) -> Result<(), String> {
        self.harness.wait_for_idle().await.map_err(|e| e.to_string())
    }

    /// Capture the final assistant reply of a completed turn, persist it, and
    /// fold it into the registry-visible output summary.
    async fn record_turn(&self, assistant: Option<&elph_ai::AssistantMessage>) {
        let text = assistant
            .map(|message| {
                let mut text = String::new();
                for block in &message.content {
                    if let elph_ai::AssistantContentBlock::Text(content) = block {
                        text.push_str(&content.text);
                    }
                }
                text
            })
            .unwrap_or_default()
            .trim()
            .to_string();

        *self.last_output.lock().await = text.clone();
        self.turns.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(dir) = &self.output_dir {
            persist::write_output(dir, &text);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_subagent_harness(
    bootstrap: &SubagentBootstrap,
    env: Arc<LocalExecutionEnv>,
    model: Model,
    models: Arc<elph_ai::Models>,
    _stream_fn: crate::types::StreamFn,
    base_tools: Vec<AgentTool>,
    active_tool_names: Vec<String>,
    root_session_id: &str,
    agent_id: &str,
    task_name: &str,
    agent_path: &str,
    depth: u32,
    _limits: SubagentLimits,
    shared_registry: Arc<AgentRegistry>,
    agent_control: Arc<AgentControl>,
    system_prompt: String,
) -> Result<Arc<SubagentHarness>, String> {
    let repo = match bootstrap.database.clone() {
        Some(db) => TursoSessionRepo::new(&bootstrap.store_db_path).with_database(db),
        None => TursoSessionRepo::new(&bootstrap.store_db_path),
    };
    let child_session_id = crate::session::id::generate_session_id();
    let session = repo
        .create(TursoSessionRepoCreateOptions {
            cwd: bootstrap.cwd.clone(),
            id: Some(child_session_id.clone()),
            parent_session_id: Some(root_session_id.to_string()),
            system_prompt: Some(system_prompt.clone()),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;

    if let Some(graph) = &bootstrap.agent_graph {
        graph
            .record_spawn(root_session_id, &child_session_id, agent_path, depth)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Persistent output/trace root: `APP_DATA/sessions/<SESSION_ID>/subagents/<agent_id>`
    // when the host configured an outputs root. Best-effort; failures never block spawn.
    let output_dir = bootstrap.ensure_output_dir(agent_id);

    #[allow(unused_mut)]
    let mut tools = base_tools;
    #[cfg(feature = "tools-collaboration")]
    if depth < _limits.max_depth {
        tools.extend(crate::tools::create_collaboration_tools(agent_control.clone()));
    }

    // `provider_id/model_id` recorded in `SubagentInfo` → `meta.json` / tool results.
    let model_ref = format!("{}/{}", model.provider, model.id);

    // Empty active_tool_names on AgentHarness means "all tools active", which would
    // force-activate every MCP tool on the child. Prefer the parent's explicit set;
    // if empty, expose native/meta tools only (MCP stays lazy-inactive).
    let active_tool_names = if active_tool_names.is_empty() {
        tools
            .iter()
            .map(|t| t.name().to_string())
            .filter(|n| !crate::collaboration::is_mcp_tool(n))
            .collect()
    } else {
        // Always keep meta tools available for discovery on the child.
        let mut names = active_tool_names;
        if !names.iter().any(|n| n == "list_available_tools")
            && tools.iter().any(|t| t.name() == "list_available_tools")
        {
            names.push("list_available_tools".into());
        }
        names
    };
    let harness = AgentHarness::new(AgentHarnessOptions {
        env,
        session,
        models,
        tools,
        resources: bootstrap.resources.clone(),
        system_prompt: SystemPrompt::Static(system_prompt),
        stream_options: bootstrap.stream_options.clone(),
        model,
        thinking_level: bootstrap.thinking_level,
        active_tool_names,
        steering_mode: QueueMode::OneAtATime,
        follow_up_mode: QueueMode::OneAtATime,
        compaction_settings: crate::agent::harness::types::DEFAULT_COMPACTION_SETTINGS,
        goal_runtime: None,
        turn_store: None,
        subagent_bootstrap: Some(bootstrap.clone()),
        shared_registry: Some(shared_registry),
        agent_control: Some(agent_control),
        headless: false,
        terminals_dir: None,
    })
    .map_err(|e: AgentHarnessError| e.to_string())?;
    harness.set_prompt_encoding(bootstrap.prompt_encoding.clone());

    let info = SubagentInfo {
        id: agent_id.to_string(),
        session_id: child_session_id,
        task_name: task_name.to_string(),
        agent_path: agent_path.to_string(),
        depth,
        status: SubagentStatus::Pending,
        parent_session_id: root_session_id.to_string(),
        model: model_ref,
        output: SubagentOutput {
            text: String::new(),
            output_path: output_dir
                .as_ref()
                .map(|dir| dir.join(persist::OUTPUT_MD).to_string_lossy().to_string()),
            finished_at_ms: None,
            turns: 0,
        },
    };

    if let Some(dir) = &output_dir {
        persist::write_meta(dir, &info);
    }

    Ok(Arc::new(SubagentHarness {
        harness: Arc::new(harness),
        info,
        output_dir,
        last_output: tokio::sync::Mutex::new(String::new()),
        turns: std::sync::atomic::AtomicU32::new(0),
        inflight: std::sync::atomic::AtomicUsize::new(0),
        turn_notify: tokio::sync::Notify::new(),
    }))
}

/// Releases an in-flight subagent turn slot when dropped.
///
/// The background turn task holds a guard for its whole lifetime, so waiters
/// are released even when the task panics, is cancelled, or is dropped early.
pub struct TurnGuard {
    harness: Arc<SubagentHarness>,
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        self.harness.turn_finished();
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

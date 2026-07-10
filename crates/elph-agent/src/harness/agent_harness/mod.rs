//! Stateful session-backed agent harness — elph-agent module.

mod accessors;
mod compaction_ops;
mod helpers;
mod hook_registration;
mod plan_mode;
mod prompt_ops;
mod run_loop;
mod setters;
mod tree_nav;

pub use helpers::NavigateTreeOptions;
use helpers::{validate_tool_names, validate_unique_names};

use std::collections::HashMap;
use std::sync::Arc;

use elph_ai::{Model, Models};
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::env::LocalExecutionEnv;
use crate::goals::GoalRuntime;
use crate::harness::hooks::HookRegistry;
use crate::harness::types::{
    AgentHarnessOptions, AgentHarnessPhase, AgentHarnessResources, AgentHarnessStreamOptions, SystemPrompt,
    clone_stream_options,
};
use crate::messages::default_convert_to_llm_fn;
use crate::mode::{CollaborationMode, filter_active_tools};
use crate::runtime::try_block_on;
use crate::session::tree::Session;
use crate::session::types::{HasSessionId, SessionStorage, SessionTreeEntry};
use crate::subagent::{AgentControl, AgentRegistry, SubagentLimits, SubagentSpawnConfig};
use crate::tools::create_multi_agent_tools;
use crate::types::{AgentMessage, AgentThinkingLevel, AgentTool, ConvertToLlmFn, QueueMode, StreamFn};

pub type HarnessOpResult<T> = std::result::Result<T, AgentHarnessError>;

use crate::harness::types::AgentHarnessError;

struct AgentHarnessTurnState {
    messages: Vec<AgentMessage>,
    resources: AgentHarnessResources,
    stream_options: AgentHarnessStreamOptions,
    session_id: String,
    system_prompt: String,
    model: Model,
    thinking_level: AgentThinkingLevel,
    _tools: Vec<AgentTool>,
    active_tools: Vec<AgentTool>,
}

struct ActiveRun {
    idle_tx: oneshot::Sender<()>,
    idle_rx: Mutex<Option<oneshot::Receiver<()>>>,
    abort_token: CancellationToken,
}

struct PendingPlanConfirmation {
    #[allow(dead_code)]
    plan_id: String,
    plan_text: String,
}

struct HarnessShared<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    env: Arc<LocalExecutionEnv>,
    session: Mutex<Session<S>>,
    models: Arc<Models>,
    phase: Mutex<AgentHarnessPhase>,
    active_run: Mutex<Option<ActiveRun>>,
    pending_session_writes: Mutex<Vec<crate::harness::types::PendingSessionWrite>>,
    model: Mutex<Model>,
    thinking_level: Mutex<AgentThinkingLevel>,
    system_prompt: Mutex<SystemPrompt<S>>,
    stream_options: Mutex<AgentHarnessStreamOptions>,
    resources: Mutex<AgentHarnessResources>,
    tools: Mutex<HashMap<String, AgentTool>>,
    active_tool_names: Mutex<Vec<String>>,
    steer_queue: Mutex<Vec<AgentMessage>>,
    steering_queue_mode: Mutex<QueueMode>,
    follow_up_queue: Mutex<Vec<AgentMessage>>,
    follow_up_queue_mode: Mutex<QueueMode>,
    next_turn_queue: Mutex<Vec<AgentMessage>>,
    hooks: HookRegistry,
    convert_to_llm: ConvertToLlmFn,
    collaboration_mode: Mutex<CollaborationMode>,
    baseline_active_tool_names: Mutex<Vec<String>>,
    pending_plan: Mutex<Option<PendingPlanConfirmation>>,
    agent_control: Mutex<Arc<AgentControl>>,
    goal_runtime: Option<Arc<GoalRuntime>>,
    subagent_bootstrap: Option<crate::subagent::SubagentBootstrap>,
}

/// Session-backed agent harness with hooks, queues, and pending session writes.
pub struct AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    shared: Arc<HarnessShared<S>>,
}

impl<S> AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    pub fn new(options: AgentHarnessOptions<S>) -> HarnessOpResult<Self> {
        let mut tools_map = HashMap::new();
        validate_unique_names(
            options.tools.iter().map(|tool| tool.name().to_string()).collect(),
            "Duplicate tool name(s)",
        )?;
        for tool in options.tools {
            tools_map.insert(tool.name().to_string(), tool);
        }

        let collaboration_mode = try_block_on(async {
            let entries = options.session.entries().await;
            let mut mode = CollaborationMode::Default;
            for entry in &entries {
                if let SessionTreeEntry::CollaborationModeChange { mode: m, .. } = entry {
                    mode = *m;
                }
            }
            mode
        })
        .unwrap_or(CollaborationMode::Default);

        let metadata = try_block_on(async { options.session.metadata().await }).map_err(|_| {
            AgentHarnessError::new(
                crate::harness::types::AgentHarnessErrorCode::InvalidState,
                "session metadata",
            )
        })?;
        let root_session_id = metadata.session_id().to_string();
        let models_for_stream = options.models.clone();
        let stream_fn: StreamFn =
            Arc::new(move |model, context, opts| models_for_stream.stream_simple(model, context, opts));
        let base_tools: Vec<AgentTool> = tools_map
            .values()
            .filter(|tool| !crate::mode::is_multi_agent_tool(tool.name()))
            .cloned()
            .collect();
        let shared_registry = options
            .shared_registry
            .clone()
            .unwrap_or_else(|| Arc::new(AgentRegistry::new()));
        let limits = SubagentLimits::default();
        let is_child_harness = options.agent_control.is_some();
        let agent_control = if let Some(control) = options.agent_control {
            control
        } else {
            Arc::new(AgentControl::new(
                SubagentSpawnConfig {
                    env: options.env.clone(),
                    model: options.model.clone(),
                    system_prompt: String::new(),
                    base_tools: base_tools.clone(),
                    stream_fn,
                    models: options.models.clone(),
                    root_session_id: root_session_id.clone(),
                    bootstrap: options.subagent_bootstrap.clone(),
                },
                limits.clone(),
                0,
                shared_registry.clone(),
                "root",
            ))
        };
        if agent_control.depth() < limits.max_depth && !is_child_harness {
            for tool in create_multi_agent_tools(agent_control.clone()) {
                tools_map.insert(tool.name().to_string(), tool);
            }
        }

        let baseline_active_tool_names: Vec<String> = if options.active_tool_names.is_empty() {
            tools_map.keys().cloned().collect()
        } else {
            options.active_tool_names
        };
        validate_unique_names(baseline_active_tool_names.clone(), "Duplicate active tool name(s)")?;
        let active_tool_names = filter_active_tools(collaboration_mode, &baseline_active_tool_names);
        validate_tool_names(&active_tool_names, &tools_map)?;

        Ok(Self {
            shared: Arc::new(HarnessShared {
                env: options.env,
                session: Mutex::new(options.session),
                models: options.models,
                phase: Mutex::new(AgentHarnessPhase::Idle),
                active_run: Mutex::new(None),
                pending_session_writes: Mutex::new(Vec::new()),
                model: Mutex::new(options.model),
                thinking_level: Mutex::new(options.thinking_level),
                system_prompt: Mutex::new(options.system_prompt),
                stream_options: Mutex::new(clone_stream_options(&options.stream_options)),
                resources: Mutex::new(options.resources),
                tools: Mutex::new(tools_map),
                active_tool_names: Mutex::new(active_tool_names),
                collaboration_mode: Mutex::new(collaboration_mode),
                baseline_active_tool_names: Mutex::new(baseline_active_tool_names),
                pending_plan: Mutex::new(None),
                agent_control: Mutex::new(agent_control),
                goal_runtime: options.goal_runtime,
                subagent_bootstrap: options.subagent_bootstrap,
                steer_queue: Mutex::new(Vec::new()),
                steering_queue_mode: Mutex::new(options.steering_mode),
                follow_up_queue: Mutex::new(Vec::new()),
                follow_up_queue_mode: Mutex::new(options.follow_up_mode),
                next_turn_queue: Mutex::new(Vec::new()),
                hooks: HookRegistry::new(),
                convert_to_llm: default_convert_to_llm_fn(),
            }),
        })
    }
}

//! Agent harness — elph-agent module.

mod accessors;
mod compaction_ops;
pub mod generic_on;
mod helpers;
mod hook_registration;
pub mod hooks;
mod plan_mode;
mod prompt_ops;
mod run_loop;
mod setters;
pub mod system_prompt;
mod tree_nav;
pub mod types;
pub mod utils;

pub use helpers::NavigateTreeOptions;
pub use hooks::{AgentHarnessEvent, HookRegistry, SUBSCRIBER_EVENT_TYPE};
pub use system_prompt::format_skills_for_system_prompt;
pub use types::{
    AbortEvent, AbortResult, AfterProviderResponseEvent, AgentHarnessError, AgentHarnessErrorCode, AgentHarnessOptions,
    AgentHarnessOwnEvent, AgentHarnessPhase, AgentHarnessPromptOptions, AgentHarnessResources,
    AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, BeforeAgentStartEvent, BeforeAgentStartResult,
    BeforeProviderPayloadEvent, BeforeProviderPayloadResult, BeforeProviderRequestEvent, BeforeProviderRequestResult,
    BranchSummaryError, BranchSummaryErrorCode, BranchSummaryResult, BranchSummarySummary, CompactResult,
    CompactionError, CompactionErrorCode, CompactionPreparation, CompactionSettings, ContextEvent, ContextResult,
    CreateDirOptions, CreateTempFileOptions, DEFAULT_COMPACTION_SETTINGS, ExecutionEnv, ExecutionError,
    ExecutionErrorCode, FileError, FileErrorCode, FileInfo, FileKind, FileOperations, FileSystem, HarnessHookResult,
    HarnessResult, ModelUpdateEvent, ModelUpdateSource, NavigateTreeResult, PendingSessionWrite, PromptTemplate,
    QueueUpdateEvent, ReadTextLinesOptions, RemoveOptions, ResourcesUpdateEvent, Result, SavePointEvent,
    SessionBeforeCompactEvent, SessionBeforeCompactResult, SessionBeforeTreeEvent, SessionBeforeTreeResult,
    SessionCompactEvent, SessionTreeEvent, SettledEvent, Shell, ShellExecOptions, ShellExecResult, Skill, SystemPrompt,
    SystemPromptContext, SystemPromptFn, ThinkingLevelUpdateEvent, ToolCallEvent, ToolCallHookResult, ToolResultEvent,
    ToolResultPatch, ToolsUpdateEvent, TreePreparation, err, get_or_throw, get_or_undefined,
    is_known_harness_hook_type, ok, to_error,
};
pub use utils::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH, ShellCaptureOptions, TruncatedBy, TruncationOptions,
    execute_shell_with_capture, finalize_shell_capture, format_size, sanitize_binary_output, truncate_head,
    truncate_line, truncate_tail,
};

use helpers::{validate_tool_names, validate_unique_names};

use std::collections::HashMap;
use std::sync::Arc;

use elph_ai::{Model, Models};
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::agent::harness::hooks::HookRegistry as HookRegistryT;
use crate::agent::harness::types::clone_stream_options;
use crate::goals::GoalRuntime;
use crate::messages::default_convert_to_llm_fn;
use crate::runtime::local_env::LocalExecutionEnv;

use crate::agent::subagent::{AgentControl, AgentRegistry, SubagentLimits, SubagentSpawnConfig, generate_agent_name};
use crate::collaboration::{CollaborationMode, filter_active_tools};
use crate::runtime::try_block_on;
use crate::session::tree::Session;
use crate::session::types::{HasSessionId, SessionStorage, SessionTreeEntry};
#[cfg(feature = "tools-collaboration")]
use crate::tools::create_collaboration_tools;
use crate::types::{AgentMessage, AgentThinkingLevel, AgentTool, ConvertToLlmFn, QueueMode, StreamFn};

pub type HarnessOpResult<T> = std::result::Result<T, AgentHarnessError>;

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
    pending_session_writes: Mutex<Vec<crate::agent::harness::types::PendingSessionWrite>>,
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
    hooks: HookRegistryT,
    convert_to_llm: ConvertToLlmFn,
    collaboration_mode: Mutex<CollaborationMode>,
    baseline_active_tool_names: Mutex<Vec<String>>,
    pending_plan: Mutex<Option<PendingPlanConfirmation>>,
    agent_control: Mutex<Arc<AgentControl>>,
    goal_runtime: Option<Arc<GoalRuntime>>,
    subagent_bootstrap: Option<crate::agent::subagent::SubagentBootstrap>,
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
                crate::agent::harness::types::AgentHarnessErrorCode::InvalidState,
                "session metadata",
            )
        })?;
        let root_session_id = metadata.session_id().to_string();
        let models_for_stream = options.models.clone();
        let stream_fn: StreamFn =
            Arc::new(move |model, context, opts| models_for_stream.stream_simple(model, context, opts));
        let base_tools: Vec<AgentTool> = tools_map
            .values()
            .filter(|tool| !crate::collaboration::is_collaboration_tool(tool.name()))
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
            {
                let parent_agent_path = generate_agent_name();
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
                    parent_agent_path,
                ))
            }
        };
        #[cfg(feature = "tools-collaboration")]
        if agent_control.depth() < limits.max_depth && !is_child_harness {
            for tool in create_collaboration_tools(agent_control.clone()) {
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
                hooks: HookRegistryT::new(),
                convert_to_llm: default_convert_to_llm_fn(),
            }),
        })
    }
}

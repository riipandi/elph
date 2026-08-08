//! Agent harness — elph-agent module.

mod accessors;
mod compaction_ops;
mod durability_ops;
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
pub use hooks::SUBSCRIBER_EVENT_TYPE;
pub use hooks::{AgentHarnessEvent, HookRegistry};
pub use system_prompt::filter_skills_for_context;
pub use system_prompt::format_skills_for_system_prompt;
pub use types::AbortEvent;
pub use types::AbortResult;
pub use types::AfterProviderResponseEvent;
pub use types::AgentHarnessError;
pub use types::AgentHarnessErrorCode;
pub use types::AgentHarnessOptions;
pub use types::AgentHarnessOwnEvent;
pub use types::AgentHarnessPhase;
pub use types::AgentHarnessPromptOptions;
pub use types::AgentHarnessResources;
pub use types::AgentHarnessStreamOptions;
pub use types::AgentHarnessStreamOptionsPatch;
pub use types::BeforeAgentStartEvent;
pub use types::BeforeAgentStartResult;
pub use types::BeforeProviderPayloadEvent;
pub use types::BeforeProviderPayloadResult;
pub use types::BeforeProviderRequestEvent;
pub use types::BeforeProviderRequestResult;
pub use types::BranchSummaryError;
pub use types::BranchSummaryErrorCode;
pub use types::BranchSummaryResult;
pub use types::BranchSummarySummary;
pub use types::CompactResult;
pub use types::CompactionError;
pub use types::CompactionErrorCode;
pub use types::CompactionPreparation;
pub use types::CompactionSettings;
pub use types::ContextEvent;
pub use types::ContextResult;
pub use types::CreateDirOptions;
pub use types::CreateTempFileOptions;
pub use types::DEFAULT_COMPACTION_SETTINGS;
pub use types::ExecutionEnv;
pub use types::ExecutionError;
pub use types::ExecutionErrorCode;
pub use types::FileError;
pub use types::FileErrorCode;
pub use types::FileInfo;
pub use types::FileKind;
pub use types::FileOperations;
pub use types::FileSystem;
pub use types::HarnessHookResult;
pub use types::HarnessResult;
pub use types::MissingActiveToolsPolicy;
pub use types::ModelUpdateEvent;
pub use types::ModelUpdateSource;
pub use types::NavigateTreeResult;
pub use types::PendingSessionWrite;
pub use types::PromptTemplate;
pub use types::QueueUpdateEvent;
pub use types::ReadTextLinesOptions;
pub use types::RecoveryPolicy;
pub use types::RemoveOptions;
pub use types::ResourcesUpdateEvent;
pub use types::RestoreOptions;
pub use types::Result;
pub use types::SavePointEvent;
pub use types::SessionBeforeCompactEvent;
pub use types::SessionBeforeCompactResult;
pub use types::SessionBeforeTreeEvent;
pub use types::SessionBeforeTreeResult;
pub use types::SessionCompactEvent;
pub use types::SessionTreeEvent;
pub use types::SettledEvent;
pub use types::Shell;
pub use types::ShellExecOptions;
pub use types::ShellExecResult;
pub use types::Skill;
pub use types::SystemPrompt;
pub use types::SystemPromptContext;
pub use types::SystemPromptFn;
pub use types::ThinkingLevelUpdateEvent;
pub use types::ToolCallEvent;
pub use types::ToolCallHookResult;
pub use types::ToolResultEvent;
pub use types::ToolResultPatch;
pub use types::ToolsUpdateEvent;
pub use types::TreePreparation;
pub use types::{err, get_or_throw, get_or_undefined, is_known_harness_hook_type, ok, to_error};
pub use utils::execute_shell_with_capture;
pub use utils::finalize_shell_capture;
pub use utils::format_size;
pub use utils::sanitize_binary_output;
pub use utils::truncate_head;
pub use utils::truncate_line;
pub use utils::truncate_tail;
pub use utils::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH};
pub use utils::{ShellCaptureOptions, TruncatedBy, TruncationOptions};

use helpers::{validate_tool_names, validate_unique_names};

use std::collections::HashMap;
use std::sync::Arc;

use elph_ai::{Model, Models};
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::agent::harness::hooks::HookRegistry as HookRegistryT;
use crate::agent::harness::types::clone_stream_options;
use crate::goals::GoalRuntime;
use crate::messages::default_convert_to_llm_fn;
use crate::prompt::encoding::PromptEncodingConfig;
use crate::runtime::local_env::LocalExecutionEnv;

use crate::agent::subagent::generate_agent_name;
use crate::agent::subagent::{AgentControl, AgentRegistry, SubagentLimits, SubagentSpawnConfig};
use crate::collaboration::CollaborationMode;
use crate::collaboration::filter_active_tools;
use crate::runtime::try_block_on;
use crate::session::tree::Session;
use crate::session::types::{HasSessionId, SessionStorage};
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
    /// Optional path to the saved plan file on disk (`.elph/plans/plan-*.md`).
    plan_file: Option<String>,
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
    /// Pending session writes with durable journal ids: `(write_id, write)`.
    pending_session_writes: Mutex<Vec<(String, crate::agent::harness::types::PendingSessionWrite)>>,
    model: Mutex<Model>,
    thinking_level: Mutex<AgentThinkingLevel>,
    system_prompt: Mutex<SystemPrompt<S>>,
    stream_options: Mutex<AgentHarnessStreamOptions>,
    resources: Mutex<AgentHarnessResources>,
    tools: Mutex<HashMap<String, AgentTool>>,
    active_tool_names: Mutex<Vec<String>>,
    /// Queues with durable journal ids: `(queue_id, message)`.
    steer_queue: Mutex<Vec<(String, AgentMessage)>>,
    steering_queue_mode: Mutex<QueueMode>,
    follow_up_queue: Mutex<Vec<(String, AgentMessage)>>,
    follow_up_queue_mode: Mutex<QueueMode>,
    next_turn_queue: Mutex<Vec<(String, AgentMessage)>>,
    hooks: HookRegistryT,
    convert_to_llm: ConvertToLlmFn,
    collaboration_mode: Mutex<CollaborationMode>,
    baseline_active_tool_names: Mutex<Vec<String>>,
    pending_plan: Mutex<Option<PendingPlanConfirmation>>,
    /// Slash prompt card metadata for the next user message write: `(kind, title)`.
    /// kind is `"skill"` or `"template"`; title is slash body without leading `/`.
    pending_prompt_meta: Mutex<Option<(String, String)>>,
    agent_control: Mutex<Arc<AgentControl>>,
    compaction_settings: CompactionSettings,
    /// TOON prompt-encoding config; `None` falls back to `ELPH_PROMPT_ENCODING*` env vars.
    prompt_encoding: std::sync::Mutex<Option<PromptEncodingConfig>>,
    goal_runtime: Option<Arc<GoalRuntime>>,
    subagent_bootstrap: Option<crate::agent::subagent::SubagentBootstrap>,
    /// Whether the harness runs in headless mode (`elph run`).
    headless: bool,
    /// Session terminal-capture directory for persisting raw tool output.
    terminals_dir: Option<std::path::PathBuf>,
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

        // Rehydrate durable harness config from session entries (semi-durable restore).
        let (restored_thinking, restored_model, restored_active_tools, collaboration_mode) = try_block_on(async {
            let entries = options.session.entries().await;
            crate::session::derive_session_context_state(&entries)
        })
        .unwrap_or((None, None, None, CollaborationMode::Default));

        let restored_model = restored_model.and_then(|r| options.models.get_model(&r.provider, &r.model_id));
        let model = restored_model.unwrap_or(options.model);
        // Only apply session thinking when an explicit ThinkingLevelChange exists.
        let thinking_level = restored_thinking
            .as_deref()
            .and_then(parse_agent_thinking_level)
            .unwrap_or(options.thinking_level);

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
            .filter(|tool| !crate::collaboration::is_collaboration_tool(tool.name(), None))
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
                        model: model.clone(),
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

        // Constructor-supplied active tools must stay strict (validate missing names).
        // Restored session names may reference tools the host no longer registers — drop those.
        let baseline_active_tool_names: Vec<String> = if let Some(names) = restored_active_tools {
            let filtered: Vec<String> = names.into_iter().filter(|n| tools_map.contains_key(n)).collect();
            if filtered.is_empty() {
                tools_map.keys().cloned().collect()
            } else {
                filtered
            }
        } else if options.active_tool_names.is_empty() {
            tools_map.keys().cloned().collect()
        } else {
            options.active_tool_names
        };
        validate_unique_names(baseline_active_tool_names.clone(), "Duplicate active tool name(s)")?;
        let active_tool_names = filter_active_tools(collaboration_mode, &baseline_active_tool_names, None);
        validate_tool_names(&active_tool_names, &tools_map)?;

        Ok(Self {
            shared: Arc::new(HarnessShared {
                env: options.env,
                session: Mutex::new(options.session),
                models: options.models,
                phase: Mutex::new(AgentHarnessPhase::Idle),
                active_run: Mutex::new(None),
                pending_session_writes: Mutex::new(Vec::new()),
                model: Mutex::new(model),
                thinking_level: Mutex::new(thinking_level),
                system_prompt: Mutex::new(options.system_prompt),
                stream_options: Mutex::new(clone_stream_options(&options.stream_options)),
                resources: Mutex::new(options.resources),
                tools: Mutex::new(tools_map),
                active_tool_names: Mutex::new(active_tool_names),
                collaboration_mode: Mutex::new(collaboration_mode),
                baseline_active_tool_names: Mutex::new(baseline_active_tool_names),
                pending_plan: Mutex::new(None),
                pending_prompt_meta: Mutex::new(None),
                agent_control: Mutex::new(agent_control),
                compaction_settings: options.compaction_settings,
                prompt_encoding: std::sync::Mutex::new(None),
                goal_runtime: options.goal_runtime,
                subagent_bootstrap: options.subagent_bootstrap,
                headless: options.headless,
                terminals_dir: options.terminals_dir,
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

    /// Open a harness with full semi-durable recovery (reconcile + rehydrate queues/pending writes).
    ///
    /// Prefer this over [`Self::new`] when resuming an existing session after process restart.
    pub async fn restore(
        mut options: AgentHarnessOptions<S>,
        restore: crate::agent::harness::types::RestoreOptions,
    ) -> HarnessOpResult<Self> {
        // 1) Repair tree (tool results) + close open operations as interrupted.
        let _ = crate::session::recovery::reconcile_session(&mut options.session).await;
        let _ = restore.recovery; // RetryUnfinished reserved; MarkInterrupted is default path.

        // 2) Fail if required restored tools missing (policy) — check before `new` drops them.
        if matches!(
            restore.missing_active_tools,
            crate::agent::harness::types::MissingActiveToolsPolicy::Fail
        ) {
            let entries = try_block_on(async { options.session.entries().await }).unwrap_or_default();
            let (_, _, restored_active, _) = crate::session::derive_session_context_state(&entries);
            if let Some(names) = restored_active {
                let registered: std::collections::HashSet<String> =
                    options.tools.iter().map(|t| t.name().to_string()).collect();
                for name in &names {
                    if !registered.contains(name) {
                        return Err(AgentHarnessError::new(
                            crate::agent::harness::types::AgentHarnessErrorCode::InvalidArgument,
                            format!("Restored active tool `{name}` is not registered by the host"),
                        ));
                    }
                }
            }
        }

        // 3) Construct harness (rehydrates model/thinking/tools/collab from session).
        let harness = Self::new(options)?;

        // 4) Rehydrate durable queues + pending writes; apply pending immediately if idle.
        harness.apply_durable_state().await?;
        if harness.phase_async().await == AgentHarnessPhase::Idle {
            let _ = harness.flush_pending_session_writes().await;
        }
        Ok(harness)
    }
}

fn parse_agent_thinking_level(s: &str) -> Option<AgentThinkingLevel> {
    match s.trim().to_ascii_lowercase().as_str() {
        "off" => Some(AgentThinkingLevel::Off),
        "minimal" => Some(AgentThinkingLevel::Minimal),
        "low" => Some(AgentThinkingLevel::Low),
        "medium" => Some(AgentThinkingLevel::Medium),
        "high" => Some(AgentThinkingLevel::High),
        "xhigh" => Some(AgentThinkingLevel::Xhigh),
        "max" => Some(AgentThinkingLevel::Max),
        _ => None,
    }
}

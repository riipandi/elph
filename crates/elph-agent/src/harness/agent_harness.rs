//! Stateful session-backed agent harness — elph-agent module.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};

use elph_ai::{
    AssistantMessage, ImageContent, Message, Model, Models, ProviderResponse, SimpleStreamOptions, StopReason,
    UserContent,
};
use serde_json::json;
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::run_agent_loop;
use crate::compaction::{
    CompactionResult as CompactionModuleResult, DEFAULT_COMPACTION_SETTINGS, GenerateBranchSummaryOptions,
    collect_entries_for_branch_summary, compact, generate_branch_summary, prepare_compaction,
};
use crate::env::LocalExecutionEnv;
use crate::harness::hooks::{AgentHarnessEvent, HookRegistry};
use crate::harness::types::{
    AbortResult, AfterProviderResponseEvent, AgentHarnessError, AgentHarnessErrorCode, AgentHarnessOptions,
    AgentHarnessOwnEvent, AgentHarnessPhase, AgentHarnessPromptOptions, AgentHarnessResources,
    AgentHarnessStreamOptions, BeforeAgentStartEvent, BeforeProviderPayloadEvent, BeforeProviderRequestEvent,
    CompactResult, CompactionError, ContextEvent, ModelUpdateSource, NavigateTreeResult, PendingSessionWrite,
    QueueUpdateEvent, SessionBeforeCompactEvent, SessionBeforeTreeEvent, SystemPrompt, ToolCallEvent, ToolResultEvent,
    clone_stream_options,
};
use crate::messages::default_convert_to_llm_fn;
use crate::prompt_templates::format_prompt_template_invocation;
use crate::runtime::try_block_on;
use crate::session::tree::{BranchSummaryOptions, Session};
use crate::session::types::{CustomMessageEntryContent, HasSessionId, SessionError, SessionStorage, SessionTreeEntry};
use crate::skills::format_skill_invocation;
use crate::types::{
    AfterToolCallResult, AgentContext, AgentEvent, AgentLoopConfig, AgentLoopTurnUpdate, AgentMessage,
    AgentThinkingLevel, AgentTool, BeforeToolCallResult, ConvertToLlmFn, GetQueuedMessagesFn, PrepareNextTurnFn,
    QueueMode, StreamFn, llm_message_to_agent,
};

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
    pending_session_writes: Mutex<Vec<PendingSessionWrite>>,
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

        let active_tool_names = if options.active_tool_names.is_empty() {
            tools_map.keys().cloned().collect()
        } else {
            options.active_tool_names
        };
        validate_unique_names(active_tool_names.clone(), "Duplicate active tool name(s)")?;
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

    pub fn env(&self) -> Arc<LocalExecutionEnv> {
        self.shared.env.clone()
    }

    pub fn models(&self) -> Arc<Models> {
        self.shared.models.clone()
    }

    pub async fn session_entries(&self) -> Vec<SessionTreeEntry> {
        self.shared.session.lock().await.entries().await
    }

    pub async fn phase(&self) -> AgentHarnessPhase {
        *self.shared.phase.lock().await
    }

    pub async fn get_model(&self) -> Model {
        self.shared.model.lock().await.clone()
    }

    pub async fn get_thinking_level(&self) -> AgentThinkingLevel {
        *self.shared.thinking_level.lock().await
    }

    pub async fn get_steering_mode(&self) -> QueueMode {
        *self.shared.steering_queue_mode.lock().await
    }

    pub async fn get_follow_up_mode(&self) -> QueueMode {
        *self.shared.follow_up_queue_mode.lock().await
    }

    pub async fn get_tools(&self) -> Vec<AgentTool> {
        self.shared.tools.lock().await.values().cloned().collect()
    }

    pub async fn get_active_tools(&self) -> Vec<AgentTool> {
        let tools = self.shared.tools.lock().await;
        let names = self.shared.active_tool_names.lock().await;
        names.iter().filter_map(|name| tools.get(name).cloned()).collect()
    }

    pub async fn get_resources(&self) -> AgentHarnessResources {
        self.shared.resources.lock().await.clone()
    }

    pub async fn get_stream_options(&self) -> AgentHarnessStreamOptions {
        let guard = self.shared.stream_options.lock().await;
        clone_stream_options(&guard)
    }

    pub async fn set_steering_mode(&self, mode: QueueMode) {
        *self.shared.steering_queue_mode.lock().await = mode;
    }

    pub async fn set_follow_up_mode(&self, mode: QueueMode) {
        *self.shared.follow_up_queue_mode.lock().await = mode;
    }

    pub async fn set_model(&self, model: Model) -> HarnessOpResult<()> {
        let previous_model = self.shared.model.lock().await.clone();
        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_model_change(&model.provider, &model.id)
                .await
                .map_err(session_error)?;
        } else {
            self.shared
                .pending_session_writes
                .lock()
                .await
                .push(PendingSessionWrite::ModelChange {
                    provider: model.provider.clone(),
                    model_id: model.id.clone(),
                });
        }
        *self.shared.model.lock().await = model.clone();
        self.emit_own(AgentHarnessOwnEvent::ModelUpdate(
            crate::harness::types::ModelUpdateEvent {
                model,
                previous_model: Some(previous_model),
                source: ModelUpdateSource::Set,
            },
        ))
        .await
    }

    pub async fn set_thinking_level(&self, level: AgentThinkingLevel) -> HarnessOpResult<()> {
        let previous_level = *self.shared.thinking_level.lock().await;
        let level_str = thinking_level_to_session_string(level);
        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_thinking_level_change(&level_str)
                .await
                .map_err(session_error)?;
        } else {
            self.shared
                .pending_session_writes
                .lock()
                .await
                .push(PendingSessionWrite::ThinkingLevelChange {
                    thinking_level: level_str,
                });
        }
        *self.shared.thinking_level.lock().await = level;
        self.emit_own(AgentHarnessOwnEvent::ThinkingLevelUpdate(
            crate::harness::types::ThinkingLevelUpdateEvent { level, previous_level },
        ))
        .await
    }

    pub async fn set_tools(
        &self,
        tools: Vec<AgentTool>,
        active_tool_names: Option<Vec<String>>,
    ) -> HarnessOpResult<()> {
        validate_unique_names(
            tools.iter().map(|t| t.name().to_string()).collect(),
            "Duplicate tool name(s)",
        )?;
        let next_tools: HashMap<_, _> = tools.iter().map(|t| (t.name().to_string(), t.clone())).collect();
        let next_active = match active_tool_names {
            Some(names) => names,
            None => self.shared.active_tool_names.lock().await.clone(),
        };
        validate_tool_names(&next_active, &next_tools)?;

        let previous_tool_names: Vec<_> = self.shared.tools.lock().await.keys().cloned().collect();
        let previous_active_tool_names = self.shared.active_tool_names.lock().await.clone();

        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_active_tools_change(next_active.clone())
                .await
                .map_err(session_error)?;
        } else {
            self.shared
                .pending_session_writes
                .lock()
                .await
                .push(PendingSessionWrite::ActiveToolsChange {
                    active_tool_names: next_active.clone(),
                });
        }

        *self.shared.tools.lock().await = next_tools;
        *self.shared.active_tool_names.lock().await = next_active.clone();
        self.emit_own(AgentHarnessOwnEvent::ToolsUpdate(
            crate::harness::types::ToolsUpdateEvent {
                tool_names: self.shared.tools.lock().await.keys().cloned().collect(),
                previous_tool_names,
                active_tool_names: next_active,
                previous_active_tool_names,
                source: ModelUpdateSource::Set,
            },
        ))
        .await
    }

    pub async fn set_active_tools(&self, tool_names: Vec<String>) -> HarnessOpResult<()> {
        let tools = self.shared.tools.lock().await;
        validate_tool_names(&tool_names, &tools)?;
        let previous_tool_names: Vec<_> = tools.keys().cloned().collect();
        let previous_active_tool_names = self.shared.active_tool_names.lock().await.clone();
        drop(tools);

        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_active_tools_change(tool_names.clone())
                .await
                .map_err(session_error)?;
        } else {
            self.shared
                .pending_session_writes
                .lock()
                .await
                .push(PendingSessionWrite::ActiveToolsChange {
                    active_tool_names: tool_names.clone(),
                });
        }

        *self.shared.active_tool_names.lock().await = tool_names.clone();
        self.emit_own(AgentHarnessOwnEvent::ToolsUpdate(
            crate::harness::types::ToolsUpdateEvent {
                tool_names: self.shared.tools.lock().await.keys().cloned().collect(),
                previous_tool_names,
                active_tool_names: tool_names,
                previous_active_tool_names,
                source: ModelUpdateSource::Set,
            },
        ))
        .await
    }

    pub async fn set_resources(&self, resources: AgentHarnessResources) -> HarnessOpResult<()> {
        let previous_resources = self.shared.resources.lock().await.clone();
        *self.shared.resources.lock().await = resources;
        self.emit_own(AgentHarnessOwnEvent::ResourcesUpdate(
            crate::harness::types::ResourcesUpdateEvent {
                resources: self.shared.resources.lock().await.clone(),
                previous_resources,
            },
        ))
        .await
    }

    pub async fn set_stream_options(&self, stream_options: AgentHarnessStreamOptions) {
        *self.shared.stream_options.lock().await = clone_stream_options(&stream_options);
    }

    pub async fn subscribe<F, Fut>(&self, listener: F) -> usize
    where
        F: Fn(AgentHarnessEvent, Option<CancellationToken>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let listener = Arc::new(move |event, signal| {
            let fut = listener(event, signal);
            Box::pin(fut) as Pin<Box<dyn Future<Output = ()> + Send>>
        });
        self.shared.hooks.subscribe(listener).await
    }

    pub async fn on_before_agent_start<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&BeforeAgentStartEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::BeforeAgentStartResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &BeforeAgentStartEvent| {
            let fut = handler(event);
            Box::pin(fut) as Pin<Box<dyn Future<Output = Option<crate::harness::types::BeforeAgentStartResult>> + Send>>
        });
        self.shared.hooks.register_before_agent_start(handler).await
    }

    pub async fn on_context<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&ContextEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HarnessOpResult<Option<crate::harness::types::ContextResult>>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &ContextEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = HarnessOpResult<Option<crate::harness::types::ContextResult>>> + Send>>
        });
        self.shared.hooks.register_context(handler).await
    }

    pub async fn on_tool_call<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&ToolCallEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::ToolCallHookResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &ToolCallEvent| {
            let fut = handler(event);
            Box::pin(fut) as Pin<Box<dyn Future<Output = Option<crate::harness::types::ToolCallHookResult>> + Send>>
        });
        self.shared.hooks.register_tool_call(handler).await
    }

    pub async fn on_tool_result<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&ToolResultEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::ToolResultPatch>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &ToolResultEvent| {
            let fut = handler(event);
            Box::pin(fut) as Pin<Box<dyn Future<Output = Option<crate::harness::types::ToolResultPatch>> + Send>>
        });
        self.shared.hooks.register_tool_result(handler).await
    }

    pub async fn on_before_provider_request<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&BeforeProviderRequestEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::BeforeProviderRequestResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &BeforeProviderRequestEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = Option<crate::harness::types::BeforeProviderRequestResult>> + Send>>
        });
        self.shared.hooks.register_before_provider_request(handler).await
    }

    pub async fn on_before_provider_payload<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&BeforeProviderPayloadEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::BeforeProviderPayloadResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &BeforeProviderPayloadEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = Option<crate::harness::types::BeforeProviderPayloadResult>> + Send>>
        });
        self.shared.hooks.register_before_provider_payload(handler).await
    }

    pub async fn on_session_before_compact<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&SessionBeforeCompactEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::SessionBeforeCompactResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &SessionBeforeCompactEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = Option<crate::harness::types::SessionBeforeCompactResult>> + Send>>
        });
        self.shared.hooks.register_session_before_compact(handler).await
    }

    pub async fn on_session_before_tree<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&SessionBeforeTreeEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::SessionBeforeTreeResult>> + Send + 'static,
    {
        let handler = Arc::new(move |event: &SessionBeforeTreeEvent| {
            let fut = handler(event);
            Box::pin(fut)
                as Pin<Box<dyn Future<Output = Option<crate::harness::types::SessionBeforeTreeResult>> + Send>>
        });
        self.shared.hooks.register_session_before_tree(handler).await
    }

    pub async fn on_after_provider_response<F, Fut>(&self, handler: F) -> usize
    where
        F: Fn(&AfterProviderResponseEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let handler = Arc::new(move |event: &AfterProviderResponseEvent| {
            let fut = handler(event);
            Box::pin(fut) as Pin<Box<dyn Future<Output = ()> + Send>>
        });
        self.shared.hooks.register_after_provider_response(handler).await
    }

    /// Upstream-compatible hook registration by snake_case event name.
    ///
    /// Mutation hooks return an optional [`HarnessHookResult`] to alter harness behavior.
    /// Observe-only hooks (for example `session_compact`, `model_update`) may return `None`.
    pub async fn on<F, Fut>(&self, event_type: &str, handler: F) -> HarnessOpResult<usize>
    where
        F: Fn(AgentHarnessOwnEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<crate::harness::types::HarnessHookResult>> + Send + 'static,
    {
        crate::harness::generic_on::register_generic_on(self, event_type, handler).await
    }

    pub(crate) fn hook_registry(&self) -> &crate::harness::hooks::HookRegistry {
        &self.shared.hooks
    }

    pub async fn prompt(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<AssistantMessage> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "AgentHarness is busy",
            ));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let result = async {
            let turn_state = self.create_turn_state().await?;
            self.execute_turn(turn_state, text.into(), options).await
        }
        .await;
        if result.is_err() {
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        }
        self.finish_run().await;
        result
    }

    pub async fn skill(&self, name: &str, additional_instructions: Option<&str>) -> HarnessOpResult<AssistantMessage> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "AgentHarness is busy",
            ));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let result = async {
            let turn_state = self.create_turn_state().await?;
            let skill = turn_state
                .resources
                .skills
                .iter()
                .find(|skill| skill.name == name)
                .ok_or_else(|| {
                    AgentHarnessError::new(AgentHarnessErrorCode::InvalidArgument, format!("Unknown skill: {name}"))
                })?;
            let text = format_skill_invocation(skill, additional_instructions);
            self.execute_turn(turn_state, text, None).await
        }
        .await;
        if result.is_err() {
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        }
        self.finish_run().await;
        result
    }

    pub async fn prompt_from_template(&self, name: &str, args: &[String]) -> HarnessOpResult<AssistantMessage> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "AgentHarness is busy",
            ));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Turn;
        self.begin_run().await;
        let result = async {
            let turn_state = self.create_turn_state().await?;
            let template = turn_state
                .resources
                .prompt_templates
                .iter()
                .find(|template| template.name == name)
                .ok_or_else(|| {
                    AgentHarnessError::new(
                        AgentHarnessErrorCode::InvalidArgument,
                        format!("Unknown prompt template: {name}"),
                    )
                })?;
            let text = format_prompt_template_invocation(template, args);
            self.execute_turn(turn_state, text, None).await
        }
        .await;
        if result.is_err() {
            *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        }
        self.finish_run().await;
        result
    }

    pub async fn steer(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<()> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Cannot steer while idle",
            ));
        }
        self.shared
            .steer_queue
            .lock()
            .await
            .push(create_user_message(text.into(), options.and_then(|o| o.images)));
        self.emit_queue_update().await
    }

    pub async fn follow_up(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<()> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Cannot follow up while idle",
            ));
        }
        self.shared
            .follow_up_queue
            .lock()
            .await
            .push(create_user_message(text.into(), options.and_then(|o| o.images)));
        self.emit_queue_update().await
    }

    pub async fn next_turn(
        &self,
        text: impl Into<String>,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<()> {
        self.shared
            .next_turn_queue
            .lock()
            .await
            .push(create_user_message(text.into(), options.and_then(|o| o.images)));
        self.emit_queue_update().await
    }

    pub async fn append_message(&self, message: AgentMessage) -> HarnessOpResult<()> {
        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_message(message)
                .await
                .map_err(session_error)?;
        } else {
            self.shared
                .pending_session_writes
                .lock()
                .await
                .push(PendingSessionWrite::Message { message });
        }
        Ok(())
    }

    pub async fn compact(&self, custom_instructions: Option<&str>) -> HarnessOpResult<CompactResult> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "compact() requires idle harness",
            ));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::Compaction;
        let result = self.compact_inner(custom_instructions).await;
        *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        result
    }

    async fn compact_inner(&self, custom_instructions: Option<&str>) -> HarnessOpResult<CompactResult> {
        let model = self.shared.model.lock().await.clone();
        let branch_entries = self
            .shared
            .session
            .lock()
            .await
            .branch(None)
            .await
            .map_err(session_error)?;
        let preparation = prepare_compaction(&branch_entries, DEFAULT_COMPACTION_SETTINGS)
            .map_err(compaction_error)?
            .ok_or_else(|| AgentHarnessError::new(AgentHarnessErrorCode::Compaction, "Nothing to compact"))?;

        let hook_result = self
            .shared
            .hooks
            .emit_session_before_compact(&SessionBeforeCompactEvent {
                preparation: preparation.clone(),
                branch_entries: branch_entries.clone(),
                custom_instructions: custom_instructions.map(str::to_string),
                abort_token: CancellationToken::new(),
            })
            .await?;

        if hook_result.as_ref().is_some_and(|r| r.cancel) {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Compaction,
                "Compaction cancelled",
            ));
        }

        let from_hook = hook_result.as_ref().and_then(|r| r.compaction.clone());
        let effective_custom_instructions = hook_result
            .as_ref()
            .and_then(|r| r.custom_instructions.clone())
            .or_else(|| custom_instructions.map(str::to_string));
        let compact_result = if let Some(result) = from_hook.clone() {
            result
        } else {
            let thinking = self.shared.thinking_level.lock().await.to_stream_reasoning();
            let module_result = compact(
                preparation,
                &self.shared.models,
                &model,
                effective_custom_instructions.as_deref(),
                None,
                thinking,
            )
            .await
            .map_err(compaction_error)?;
            module_to_compact_result(module_result)
        };

        let entry_id = self
            .shared
            .session
            .lock()
            .await
            .append_compaction(
                &compact_result.summary,
                &compact_result.first_kept_entry_id,
                compact_result.tokens_before,
                compact_result.details.clone(),
                Some(from_hook.is_some()),
            )
            .await
            .map_err(session_error)?;

        if let Some(entry) = self.shared.session.lock().await.entry(&entry_id).await
            && matches!(entry, SessionTreeEntry::Compaction { .. })
        {
            self.emit_own(AgentHarnessOwnEvent::SessionCompact(
                crate::harness::types::SessionCompactEvent {
                    compaction_entry: entry,
                    from_hook: from_hook.is_some(),
                },
            ))
            .await?;
        }

        Ok(compact_result)
    }

    pub async fn navigate_tree(
        &self,
        target_id: &str,
        options: Option<NavigateTreeOptions>,
    ) -> HarnessOpResult<NavigateTreeResult> {
        if self.phase_async().await != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "navigate_tree() requires idle harness",
            ));
        }
        *self.shared.phase.lock().await = AgentHarnessPhase::BranchSummary;
        let result = self.navigate_tree_inner(target_id, options).await;
        *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
        result
    }

    async fn navigate_tree_inner(
        &self,
        target_id: &str,
        options: Option<NavigateTreeOptions>,
    ) -> HarnessOpResult<NavigateTreeResult> {
        let options = options.unwrap_or_default();
        let old_leaf_id = self
            .shared
            .session
            .lock()
            .await
            .leaf_id()
            .await
            .map_err(session_error)?;
        if old_leaf_id.as_deref() == Some(target_id) {
            return Ok(NavigateTreeResult {
                cancelled: false,
                editor_text: None,
                summary_entry: None,
            });
        }

        let target_entry = self.shared.session.lock().await.entry(target_id).await.ok_or_else(|| {
            AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidArgument,
                format!("Entry {target_id} not found"),
            )
        })?;

        let collected = {
            let session = self.shared.session.lock().await;
            collect_entries_for_branch_summary(&session, old_leaf_id.as_deref(), target_id)
                .await
                .map_err(session_error)?
        };

        let preparation = crate::harness::types::TreePreparation {
            target_id: target_id.to_string(),
            old_leaf_id: old_leaf_id.clone(),
            common_ancestor_id: collected.common_ancestor_id.clone(),
            entries_to_summarize: collected.entries.clone(),
            user_wants_summary: options.summarize,
            custom_instructions: options.custom_instructions.clone(),
            replace_instructions: options.replace_instructions,
            label: options.label.clone(),
        };

        let hook_result = self
            .shared
            .hooks
            .emit_session_before_tree(&SessionBeforeTreeEvent {
                preparation: preparation.clone(),
                abort_token: CancellationToken::new(),
            })
            .await?;

        if hook_result.as_ref().is_some_and(|r| r.cancel) {
            return Ok(NavigateTreeResult {
                cancelled: true,
                editor_text: None,
                summary_entry: None,
            });
        }

        let mut summary_text = hook_result
            .as_ref()
            .and_then(|r| r.summary.as_ref())
            .map(|s| s.summary.clone());
        let mut summary_details = hook_result
            .as_ref()
            .and_then(|r| r.summary.as_ref())
            .and_then(|s| s.details.clone());

        if summary_text.is_none() && options.summarize && !collected.entries.is_empty() {
            let model = self.shared.model.lock().await.clone();
            let branch_summary = generate_branch_summary(
                &collected.entries,
                &self.shared.models,
                &model,
                GenerateBranchSummaryOptions {
                    custom_instructions: hook_result
                        .as_ref()
                        .and_then(|r| r.custom_instructions.clone())
                        .or(options.custom_instructions.clone()),
                    replace_instructions: hook_result
                        .as_ref()
                        .map(|r| r.replace_instructions)
                        .unwrap_or(options.replace_instructions),
                    ..Default::default()
                },
            )
            .await
            .map_err(branch_summary_error)?;

            summary_text = Some(branch_summary.summary);
            summary_details = Some(json!({
                "readFiles": branch_summary.read_files,
                "modifiedFiles": branch_summary.modified_files,
            }));
        }

        let (new_leaf_id, editor_text) = editor_state_for_target(&target_entry);

        let summary_id = self
            .shared
            .session
            .lock()
            .await
            .move_to(
                new_leaf_id.as_deref(),
                summary_text.as_ref().map(|summary| BranchSummaryOptions {
                    summary: summary.clone(),
                    details: summary_details.clone(),
                    from_hook: Some(hook_result.as_ref().and_then(|r| r.summary.as_ref()).is_some()),
                }),
            )
            .await
            .map_err(session_error)?;

        let summary_entry = if let Some(summary_id) = summary_id {
            self.shared.session.lock().await.entry(&summary_id).await
        } else {
            None
        };

        let new_leaf = self
            .shared
            .session
            .lock()
            .await
            .leaf_id()
            .await
            .map_err(session_error)?;
        self.emit_own(AgentHarnessOwnEvent::SessionTree(
            crate::harness::types::SessionTreeEvent {
                new_leaf_id: new_leaf,
                old_leaf_id,
                summary_entry: summary_entry.clone(),
                from_hook: Some(hook_result.as_ref().and_then(|r| r.summary.as_ref()).is_some()),
            },
        ))
        .await?;

        Ok(NavigateTreeResult {
            cancelled: false,
            editor_text,
            summary_entry,
        })
    }

    pub async fn abort(&self) -> HarnessOpResult<AbortResult> {
        let cleared_steer: Vec<AgentMessage> = self.shared.steer_queue.lock().await.drain(..).collect();
        let cleared_follow_up: Vec<AgentMessage> = self.shared.follow_up_queue.lock().await.drain(..).collect();
        if let Some(run) = self.shared.active_run.lock().await.as_ref() {
            run.abort_token.cancel();
        }

        let mut errors = Vec::new();
        if let Err(error) = self.emit_queue_update().await {
            errors.push(error.to_string());
        }
        if let Err(error) = self.wait_for_idle().await {
            errors.push(error.to_string());
        }
        if let Err(error) = self
            .emit_own(AgentHarnessOwnEvent::Abort(crate::harness::types::AbortEvent {
                cleared_steer: cleared_steer.clone(),
                cleared_follow_up: cleared_follow_up.clone(),
            }))
            .await
        {
            errors.push(error.to_string());
        }

        if !errors.is_empty() {
            return Err(AgentHarnessError::new(AgentHarnessErrorCode::Hook, errors.join("; ")));
        }

        Ok(AbortResult {
            cleared_steer,
            cleared_follow_up,
        })
    }

    pub async fn wait_for_idle(&self) -> HarnessOpResult<()> {
        let rx = {
            let guard = self.shared.active_run.lock().await;
            if let Some(run) = guard.as_ref() {
                run.idle_rx.lock().await.take()
            } else {
                None
            }
        };
        if let Some(rx) = rx {
            let _ = rx.await;
        }
        Ok(())
    }

    async fn phase_async(&self) -> AgentHarnessPhase {
        *self.shared.phase.lock().await
    }

    async fn begin_run(&self) {
        let (idle_tx, idle_rx) = oneshot::channel();
        let abort_token = CancellationToken::new();
        *self.shared.active_run.lock().await = Some(ActiveRun {
            idle_tx,
            idle_rx: Mutex::new(Some(idle_rx)),
            abort_token,
        });
    }

    async fn finish_run(&self) {
        if let Some(run) = self.shared.active_run.lock().await.take() {
            let _ = run.idle_tx.send(());
        }
    }

    async fn emit_run_failure(
        &self,
        model: &Model,
        error: &str,
        aborted: bool,
        _emit: &crate::agent_loop::AgentEventCallback,
    ) -> HarnessOpResult<AssistantMessage> {
        let failure_message = llm_message_to_agent(create_failure_message(model, error, aborted));
        self.handle_agent_event(
            AgentEvent::MessageStart {
                message: failure_message.clone(),
            },
            None,
        )
        .await?;
        self.handle_agent_event(
            AgentEvent::MessageEnd {
                message: failure_message.clone(),
            },
            None,
        )
        .await?;
        self.handle_agent_event(
            AgentEvent::TurnEnd {
                message: failure_message.clone(),
                tool_results: Vec::new(),
            },
            None,
        )
        .await?;
        self.handle_agent_event(
            AgentEvent::AgentEnd {
                messages: vec![failure_message.clone()],
            },
            None,
        )
        .await?;
        self.flush_pending_session_writes().await?;
        match failure_message.as_llm() {
            Some(Message::Assistant(assistant)) => Ok(assistant.clone()),
            _ => Err(AgentHarnessError::new(
                AgentHarnessErrorCode::InvalidState,
                "Failure message was not an assistant message",
            )),
        }
    }

    async fn create_turn_state(&self) -> HarnessOpResult<AgentHarnessTurnState> {
        let session = self.shared.session.lock().await;
        let context = session.build_context().await.map_err(session_error)?;
        let metadata = session.metadata().await;
        drop(session);

        let resources = self.shared.resources.lock().await.clone();
        let tools = self.shared.tools.lock().await.values().cloned().collect();
        let active_tools: Vec<AgentTool> = {
            let tools = self.shared.tools.lock().await;
            let names = self.shared.active_tool_names.lock().await;
            names.iter().filter_map(|name| tools.get(name).cloned()).collect()
        };
        let model = self.shared.model.lock().await.clone();
        let thinking_level = *self.shared.thinking_level.lock().await;
        let stream_options = {
            let guard = self.shared.stream_options.lock().await;
            clone_stream_options(&guard)
        };

        let system_prompt = match &*self.shared.system_prompt.lock().await {
            SystemPrompt::Static(text) => text.clone(),
            SystemPrompt::Dynamic(func) => {
                let session = self.shared.session.lock().await.clone();
                let ctx = crate::harness::types::SystemPromptContext {
                    env: self.shared.env.clone(),
                    session,
                    model: model.clone(),
                    thinking_level,
                    active_tools: active_tools.clone(),
                    resources: resources.clone(),
                };
                func(ctx).await
            }
        };

        Ok(AgentHarnessTurnState {
            messages: context.messages,
            resources,
            stream_options,
            session_id: metadata.session_id().to_string(),
            system_prompt,
            model,
            thinking_level,
            _tools: tools,
            active_tools,
        })
    }

    async fn execute_turn(
        &self,
        turn_state: AgentHarnessTurnState,
        text: String,
        options: Option<AgentHarnessPromptOptions>,
    ) -> HarnessOpResult<AssistantMessage> {
        let images = options.as_ref().and_then(|o| o.images.clone());
        let mut messages = vec![create_user_message(text.clone(), images.clone())];

        if !self.shared.next_turn_queue.lock().await.is_empty() {
            let queued = self.shared.next_turn_queue.lock().await.drain(..).collect::<Vec<_>>();
            if let Err(error) = self.emit_queue_update().await {
                *self.shared.next_turn_queue.lock().await = queued;
                return Err(error);
            }
            let prompt = messages.pop().expect("prompt message");
            messages = queued;
            messages.push(prompt);
        }

        let before_result = self
            .shared
            .hooks
            .emit_before_agent_start(&BeforeAgentStartEvent {
                prompt: text,
                images: images.clone(),
                system_prompt: turn_state.system_prompt.clone(),
                resources: turn_state.resources.clone(),
            })
            .await?;

        if let Some(extra) = before_result.as_ref().and_then(|r| r.messages.clone()) {
            messages.extend(extra);
        }

        let abort_token = {
            let guard = self.shared.active_run.lock().await;
            guard
                .as_ref()
                .map(|run| run.abort_token.clone())
                .unwrap_or_else(CancellationToken::new)
        };

        let turn_state = Arc::new(StdMutex::new(turn_state));
        let system_prompt_override = before_result.and_then(|r| r.system_prompt);
        let context = self.create_context(
            &turn_state.lock().expect("turn state lock"),
            system_prompt_override.as_deref(),
        );
        let config = self.create_loop_config(turn_state.clone());
        let shared = self.shared.clone();

        let emit_token = abort_token.clone();
        let emit: crate::agent_loop::AgentEventCallback = Arc::new(move |event| {
            let shared = shared.clone();
            let token = emit_token.clone();
            Box::pin(async move {
                let harness = AgentHarness { shared: shared.clone() };
                let _ = harness.handle_agent_event(event, Some(token)).await;
            })
        });

        let run_result = match run_agent_loop(messages, context, config, emit.clone(), Some(abort_token.clone())).await
        {
            Ok(messages) => messages,
            Err(error) => {
                let model = turn_state.lock().expect("turn state lock").model.clone();
                return self
                    .emit_run_failure(&model, &error, abort_token.is_cancelled(), &emit)
                    .await;
            }
        };

        self.flush_pending_session_writes().await?;

        for message in run_result.into_iter().rev() {
            if let Some(assistant) = message.as_llm()
                && let Message::Assistant(assistant) = assistant
            {
                return Ok(assistant.clone());
            }
        }

        Err(AgentHarnessError::new(
            AgentHarnessErrorCode::InvalidState,
            "AgentHarness prompt completed without an assistant message",
        ))
    }

    fn create_context(&self, turn_state: &AgentHarnessTurnState, system_prompt: Option<&str>) -> AgentContext {
        AgentContext {
            system_prompt: system_prompt.unwrap_or(&turn_state.system_prompt).to_string(),
            messages: turn_state.messages.clone(),
            tools: turn_state.active_tools.clone(),
        }
    }

    fn create_loop_config(&self, turn_state: Arc<StdMutex<AgentHarnessTurnState>>) -> AgentLoopConfig {
        let shared = self.shared.clone();
        let snapshot = turn_state.lock().expect("turn state lock");
        let thinking_level = snapshot.thinking_level;
        let model = snapshot.model.clone();
        drop(snapshot);

        let get_steering: GetQueuedMessagesFn = {
            let shared = shared.clone();
            Arc::new(move || {
                let shared = shared.clone();
                Box::pin(async move {
                    let harness = AgentHarness { shared };
                    harness.drain_queued_messages(true).await
                })
            })
        };
        let get_follow_up: GetQueuedMessagesFn = {
            let shared = shared.clone();
            Arc::new(move || {
                let shared = shared.clone();
                Box::pin(async move {
                    let harness = AgentHarness { shared };
                    harness.drain_queued_messages(false).await
                })
            })
        };

        let prepare_shared = shared.clone();
        let prepare_turn_state = turn_state.clone();
        let prepare_next_turn: Option<PrepareNextTurnFn> = Some(Arc::new(move |_| {
            let shared = prepare_shared.clone();
            let turn_state = prepare_turn_state.clone();
            Box::pin(async move {
                let harness = AgentHarness { shared };
                harness.flush_pending_session_writes().await.ok()?;
                let next = harness.create_turn_state().await.ok()?;
                *turn_state.lock().expect("turn state lock") = next;
                let snapshot = turn_state.lock().expect("turn state lock");
                Some(AgentLoopTurnUpdate {
                    context: Some(harness.create_context(&snapshot, None)),
                    model: Some(snapshot.model.clone()),
                    thinking_level: Some(snapshot.thinking_level),
                })
            })
        }));

        let hooks = Arc::new(self.shared.hooks.clone_shallow());
        let before_tool_call: Option<crate::types::BeforeToolCallFn> =
            Some(Arc::new(move |ctx: crate::types::BeforeToolCallContext, _| {
                let hooks = hooks.clone();
                let event = ToolCallEvent {
                    tool_call_id: ctx.tool_call.id.clone(),
                    tool_name: ctx.tool_call.name.clone(),
                    input: ctx.args.clone(),
                };
                Box::pin(async move {
                    let result = hooks.emit_tool_call(&event).await.ok()??;
                    Some(BeforeToolCallResult {
                        block: result.block,
                        reason: result.reason,
                        args: None,
                    })
                })
            }));
        let hooks = Arc::new(self.shared.hooks.clone_shallow());
        let after_tool_call: Option<crate::types::AfterToolCallFn> =
            Some(Arc::new(move |ctx: crate::types::AfterToolCallContext, _| {
                let hooks = hooks.clone();
                let event = ToolResultEvent {
                    tool_call_id: ctx.tool_call.id.clone(),
                    tool_name: ctx.tool_call.name.clone(),
                    input: ctx.args.clone(),
                    content: ctx.result.content.clone(),
                    details: ctx.result.details.clone(),
                    is_error: ctx.is_error,
                };
                Box::pin(async move {
                    let result = hooks.emit_tool_result(&event).await.ok()??;
                    Some(AfterToolCallResult {
                        content: result.content,
                        details: result.details,
                        is_error: result.is_error,
                        terminate: result.terminate,
                    })
                })
            }));

        let transform_shared = shared.clone();
        let transform_context: Option<crate::types::TransformContextFn> =
            Some(Arc::new(move |messages: Vec<AgentMessage>, _| {
                let shared = transform_shared.clone();
                Box::pin(async move {
                    let harness = AgentHarness { shared };
                    let event = ContextEvent {
                        messages: messages.clone(),
                    };
                    match harness.shared.hooks.emit_context(&event).await {
                        Ok(Some(result)) => Ok(result.messages),
                        Ok(None) => Ok(messages),
                        Err(error) => Err(error.to_string()),
                    }
                })
            }));

        let stream_options = SimpleStreamOptions {
            base: Default::default(),
            reasoning: thinking_level.to_stream_reasoning(),
            thinking_budgets: None,
        };

        AgentLoopConfig {
            model,
            stream_options,
            convert_to_llm: self.shared.convert_to_llm.clone(),
            transform_context,
            get_api_key: None,
            should_stop_after_turn: None,
            prepare_next_turn,
            get_steering_messages: Some(get_steering),
            get_follow_up_messages: Some(get_follow_up),
            tool_execution: crate::types::ToolExecutionMode::Parallel,
            before_tool_call,
            after_tool_call,
            stream_fn: Some(self.create_stream_fn(turn_state)),
        }
    }

    fn create_stream_fn(&self, turn_state: Arc<StdMutex<AgentHarnessTurnState>>) -> StreamFn {
        let models = self.shared.models.clone();
        let hooks = self.shared.hooks.clone();
        Arc::new(move |model, context, options| {
            let (mut snapshot, session_id) = {
                let turn_state = turn_state.lock().expect("turn state lock");
                (
                    clone_stream_options(&turn_state.stream_options),
                    turn_state.session_id.clone(),
                )
            };

            if let Ok(Ok(merged)) = try_block_on(hooks.emit_before_provider_request(&BeforeProviderRequestEvent {
                model: model.clone(),
                session_id: session_id.clone(),
                stream_options: clone_stream_options(&snapshot),
            })) {
                snapshot = merged;
            }

            let hooks_for_payload = hooks.clone();
            let mut simple = merge_harness_into_simple(options, &snapshot, &session_id);
            let existing_on_payload = simple.base.on_payload.take();
            simple.base.on_payload = Some(Arc::new(move |payload, model_ref| {
                let hooks = hooks_for_payload.clone();
                let existing = existing_on_payload.clone();
                Box::pin(async move {
                    let mut current = payload;
                    if let Ok(transformed) = hooks
                        .emit_before_provider_payload(&BeforeProviderPayloadEvent {
                            model: model_ref.clone(),
                            payload: current.clone(),
                        })
                        .await
                    {
                        current = transformed;
                    }
                    if let Some(previous) = existing {
                        let input = current.clone();
                        if let Some(transformed) = previous(input, model_ref).await {
                            current = transformed;
                        }
                    }
                    Some(current)
                })
            }));

            let hooks_for_response = hooks.clone();
            let existing_on_response = simple.base.on_response.take();
            simple.base.on_response = Some(Arc::new(move |response: ProviderResponse, model_ref| {
                let hooks = hooks_for_response.clone();
                let existing = existing_on_response.clone();
                Box::pin(async move {
                    hooks
                        .emit_after_provider_response(&AfterProviderResponseEvent {
                            status: response.status,
                            headers: response.headers.clone(),
                        })
                        .await;
                    if let Some(previous) = existing {
                        previous(response, model_ref).await;
                    }
                })
            }));

            models.stream_simple(model, context, Some(simple))
        })
    }

    async fn drain_queued_messages(&self, steering: bool) -> Vec<AgentMessage> {
        if steering {
            self.drain_queue(
                &self.shared.steer_queue,
                *self.shared.steering_queue_mode.lock().await,
                true,
            )
            .await
        } else {
            self.drain_queue(
                &self.shared.follow_up_queue,
                *self.shared.follow_up_queue_mode.lock().await,
                false,
            )
            .await
        }
    }

    async fn drain_queue(
        &self,
        queue: &Mutex<Vec<AgentMessage>>,
        mode: QueueMode,
        is_steer: bool,
    ) -> Vec<AgentMessage> {
        let count = {
            let guard = queue.lock().await;
            if mode == QueueMode::All {
                guard.len()
            } else {
                1.min(guard.len())
            }
        };
        let messages: Vec<_> = queue.lock().await.drain(..count).collect();
        if messages.is_empty() {
            return messages;
        }
        if let Err(error) = self.emit_queue_update().await {
            let mut guard = queue.lock().await;
            for message in messages.into_iter().rev() {
                guard.insert(0, message);
            }
            let _ = (error, is_steer);
            return Vec::new();
        }
        messages
    }

    async fn flush_pending_session_writes(&self) -> HarnessOpResult<()> {
        loop {
            let write = self.shared.pending_session_writes.lock().await.first().cloned();
            let Some(write) = write else { break };
            match write {
                PendingSessionWrite::Message { message } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_message(message)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::ModelChange { provider, model_id } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_model_change(&provider, &model_id)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::ThinkingLevelChange { thinking_level } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_thinking_level_change(&thinking_level)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::ActiveToolsChange { active_tool_names } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_active_tools_change(active_tool_names)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::Custom { custom_type, data } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_custom_entry(&custom_type, data)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::CustomMessage {
                    custom_type,
                    content,
                    display,
                    details,
                } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_custom_message_entry(&custom_type, content, display, details)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::Label { target_id, label } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_label(&target_id, label.as_deref())
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::SessionInfo { name } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .append_session_name(name.unwrap_or_default())
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::Leaf { target_id } => {
                    self.shared
                        .session
                        .lock()
                        .await
                        .storage_mut()
                        .set_leaf_id(target_id)
                        .await
                        .map_err(session_error)?;
                }
                PendingSessionWrite::Compaction { .. } | PendingSessionWrite::BranchSummary { .. } => {}
            }
            self.shared.pending_session_writes.lock().await.remove(0);
        }
        Ok(())
    }

    async fn handle_agent_event(&self, event: AgentEvent, signal: Option<CancellationToken>) -> HarnessOpResult<()> {
        match &event {
            AgentEvent::MessageEnd { message } => {
                self.shared
                    .session
                    .lock()
                    .await
                    .append_message(message.clone())
                    .await
                    .map_err(session_error)?;
                self.shared
                    .hooks
                    .emit_subscriber(AgentHarnessEvent::Agent(event.clone()), signal)
                    .await?;
                return Ok(());
            }
            AgentEvent::TurnEnd { .. } => {
                let event_error = self
                    .shared
                    .hooks
                    .emit_subscriber(AgentHarnessEvent::Agent(event.clone()), signal.clone())
                    .await
                    .err();
                let had_pending = !self.shared.pending_session_writes.lock().await.is_empty();
                self.flush_pending_session_writes().await?;
                if let Some(error) = event_error {
                    return Err(error);
                }
                self.emit_own(AgentHarnessOwnEvent::SavePoint(crate::harness::types::SavePointEvent {
                    had_pending_mutations: had_pending,
                }))
                .await?;
                return Ok(());
            }
            AgentEvent::AgentEnd { .. } => {
                self.flush_pending_session_writes().await?;
                *self.shared.phase.lock().await = AgentHarnessPhase::Idle;
                self.shared
                    .hooks
                    .emit_subscriber(AgentHarnessEvent::Agent(event.clone()), signal.clone())
                    .await?;
                let next_turn_count = self.shared.next_turn_queue.lock().await.len();
                self.emit_own(AgentHarnessOwnEvent::Settled(crate::harness::types::SettledEvent {
                    next_turn_count,
                }))
                .await?;
                return Ok(());
            }
            _ => {}
        }
        self.shared
            .hooks
            .emit_subscriber(AgentHarnessEvent::Agent(event.clone()), signal)
            .await
    }

    async fn emit_own(&self, event: AgentHarnessOwnEvent) -> HarnessOpResult<()> {
        self.shared
            .hooks
            .emit_subscriber(AgentHarnessEvent::Own(event), None)
            .await
    }

    async fn emit_queue_update(&self) -> HarnessOpResult<()> {
        self.emit_own(AgentHarnessOwnEvent::QueueUpdate(QueueUpdateEvent {
            steer: self.shared.steer_queue.lock().await.clone(),
            follow_up: self.shared.follow_up_queue.lock().await.clone(),
            next_turn: self.shared.next_turn_queue.lock().await.clone(),
        }))
        .await
    }
}

/// Options for [`AgentHarness::navigate_tree`].
#[derive(Debug, Clone, Default)]
pub struct NavigateTreeOptions {
    pub summarize: bool,
    pub custom_instructions: Option<String>,
    pub replace_instructions: bool,
    pub label: Option<String>,
}

fn create_failure_message(model: &Model, error: &str, aborted: bool) -> Message {
    Message::Assistant(AssistantMessage {
        role: "assistant".to_string(),
        content: vec![elph_ai::AssistantContentBlock::Text(elph_ai::TextContent::new(""))],
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        response_model: None,
        response_id: None,
        usage: elph_ai::Usage::default(),
        stop_reason: if aborted {
            StopReason::Aborted
        } else {
            StopReason::Error
        },
        error_message: Some(error.to_string()),
        timestamp: now_ms(),
    })
}

fn create_user_message(text: String, images: Option<Vec<ImageContent>>) -> AgentMessage {
    let mut content = vec![elph_ai::ContentBlock::Text { text }];
    if let Some(images) = images {
        for image in images {
            content.push(elph_ai::ContentBlock::Image {
                data: image.data,
                mime_type: image.mime_type,
            });
        }
    }
    llm_message_to_agent(Message::User {
        content: UserContent::Blocks(content),
        timestamp: now_ms(),
    })
}

fn merge_harness_into_simple(
    options: Option<SimpleStreamOptions>,
    harness: &AgentHarnessStreamOptions,
    session_id: &str,
) -> SimpleStreamOptions {
    let mut simple = options.unwrap_or(SimpleStreamOptions {
        base: Default::default(),
        reasoning: None,
        thinking_budgets: None,
    });
    if let Some(transport) = harness.transport {
        simple.base.transport = Some(transport);
    }
    if let Some(timeout_ms) = harness.timeout_ms {
        simple.base.timeout_ms = Some(timeout_ms);
    }
    if let Some(max_retries) = harness.max_retries {
        simple.base.max_retries = Some(max_retries);
    }
    if let Some(max_retry_delay_ms) = harness.max_retry_delay_ms {
        simple.base.max_retry_delay_ms = Some(max_retry_delay_ms);
    }
    if let Some(headers) = &harness.headers {
        simple.base.headers = Some(headers.iter().map(|(k, v)| (k.clone(), Some(v.clone()))).collect());
    }
    if let Some(metadata) = &harness.metadata
        && let serde_json::Value::Object(map) = metadata
    {
        simple.base.metadata = Some(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    }
    simple.base.session_id = Some(session_id.to_string());
    simple
}

fn find_duplicate_names(names: &[String]) -> Vec<String> {
    let mut seen = HashMap::new();
    let mut duplicates = Vec::new();
    for name in names {
        if seen.insert(name.clone(), true).is_some() {
            duplicates.push(name.clone());
        }
    }
    duplicates
}

fn validate_unique_names(names: Vec<String>, message: &str) -> HarnessOpResult<()> {
    let duplicates = find_duplicate_names(&names);
    if !duplicates.is_empty() {
        return Err(AgentHarnessError::new(
            AgentHarnessErrorCode::InvalidArgument,
            format!("{message}: {}", duplicates.join(", ")),
        ));
    }
    Ok(())
}

fn validate_tool_names(tool_names: &[String], tools: &HashMap<String, AgentTool>) -> HarnessOpResult<()> {
    validate_unique_names(tool_names.to_vec(), "Duplicate active tool name(s)")?;
    let missing: Vec<_> = tool_names.iter().filter(|name| !tools.contains_key(*name)).collect();
    if !missing.is_empty() {
        return Err(AgentHarnessError::new(
            AgentHarnessErrorCode::InvalidArgument,
            format!(
                "Unknown tool(s): {}",
                missing.into_iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    Ok(())
}

fn thinking_level_to_session_string(level: AgentThinkingLevel) -> String {
    match level {
        AgentThinkingLevel::Off => "off".to_string(),
        AgentThinkingLevel::Minimal => "minimal".to_string(),
        AgentThinkingLevel::Low => "low".to_string(),
        AgentThinkingLevel::Medium => "medium".to_string(),
        AgentThinkingLevel::High => "high".to_string(),
        AgentThinkingLevel::Xhigh => "xhigh".to_string(),
    }
}

fn module_to_compact_result(result: CompactionModuleResult) -> CompactResult {
    CompactResult {
        summary: result.summary,
        first_kept_entry_id: result.first_kept_entry_id,
        tokens_before: result.tokens_before,
        details: result.details.map(|details| {
            json!({
                "readFiles": details.read_files,
                "modifiedFiles": details.modified_files,
            })
        }),
    }
}

fn editor_state_for_target(entry: &SessionTreeEntry) -> (Option<String>, Option<String>) {
    match entry {
        SessionTreeEntry::Message { message, parent_id, .. } if message.role() == "user" => {
            let editor_text = user_message_text(message);
            (parent_id.clone(), editor_text)
        }
        SessionTreeEntry::CustomMessage { content, parent_id, .. } => {
            let editor_text = match content {
                CustomMessageEntryContent::Text(text) => Some(text.clone()),
                CustomMessageEntryContent::Blocks(blocks) => {
                    let text = blocks
                        .iter()
                        .filter_map(|block| match block {
                            crate::session::types::CustomMessageEntryBlock::Text(text) => Some(text.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if text.is_empty() { None } else { Some(text) }
                }
            };
            (parent_id.clone(), editor_text)
        }
        _ => (Some(entry.id().to_string()), None),
    }
}

fn user_message_text(message: &AgentMessage) -> Option<String> {
    let llm = message.as_llm()?;
    let Message::User { content, .. } = llm else {
        return None;
    };
    match content {
        UserContent::Text(text) => Some(text.clone()),
        UserContent::Blocks(blocks) => {
            let text = blocks
                .iter()
                .filter_map(|block| match block {
                    elph_ai::ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

fn session_error(error: SessionError) -> AgentHarnessError {
    AgentHarnessError::new(AgentHarnessErrorCode::Session, error.to_string())
}

fn compaction_error(error: CompactionError) -> AgentHarnessError {
    AgentHarnessError::new(AgentHarnessErrorCode::Compaction, error.to_string())
}

fn branch_summary_error(error: crate::harness::types::BranchSummaryError) -> AgentHarnessError {
    AgentHarnessError::new(AgentHarnessErrorCode::BranchSummary, error.to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

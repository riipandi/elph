//! Agent harness configuration setters.

use std::collections::HashMap;

use elph_ai::Model;

use crate::agent::harness::types::AgentHarnessOwnEvent;
use crate::agent::harness::types::AgentHarnessPhase;
use crate::agent::harness::types::AgentHarnessResources;
use crate::agent::harness::types::AgentHarnessStreamOptions;
use crate::agent::harness::types::ModelUpdateSource;
use crate::agent::harness::types::PendingSessionWrite;
use crate::agent::harness::types::SystemPrompt;
use crate::agent::harness::types::clone_stream_options;
use crate::prompt::encoding::PromptEncodingConfig;
use crate::types::{AgentThinkingLevel, AgentTool};

use super::helpers::{session_error, thinking_level_to_session_string, validate_tool_names, validate_unique_names};
use super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: crate::session::types::SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: crate::session::types::HasSessionId + Send + Sync,
{
    pub async fn set_steering_mode(&self, mode: crate::types::QueueMode) {
        *self.shared.steering_queue_mode.lock().await = mode;
    }

    pub async fn set_follow_up_mode(&self, mode: crate::types::QueueMode) {
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
            let write = PendingSessionWrite::ModelChange {
                provider: model.provider.clone(),
                model_id: model.id.clone(),
            };
            self.enqueue_pending_write(write).await?;
        }
        *self.shared.model.lock().await = model.clone();
        // Keep the subagent (AgentControl) model in sync so a child spawned after a
        // model switch inherits the active selection, never the `defaultModel` captured
        // at harness construction. `create_turn_state` also refreshes this at turn start.
        self.shared.agent_control.lock().await.set_model(model.clone()).await;
        self.emit_own(AgentHarnessOwnEvent::ModelUpdate(
            crate::agent::harness::types::ModelUpdateEvent {
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
            let write = PendingSessionWrite::ThinkingLevelChange {
                thinking_level: level_str,
            };
            self.enqueue_pending_write(write).await?;
        }
        *self.shared.thinking_level.lock().await = level;
        self.emit_own(AgentHarnessOwnEvent::ThinkingLevelUpdate(
            crate::agent::harness::types::ThinkingLevelUpdateEvent { level, previous_level },
        ))
        .await
    }

    pub async fn set_tools(
        &self,
        tools: Vec<AgentTool>,
        active_tool_names: Option<Vec<String>>,
    ) -> HarnessOpResult<()> {
        validate_unique_names(tools.iter().map(|t| t.name().to_string()).collect(), "Duplicate tool name(s)")?;
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
            let write = PendingSessionWrite::ActiveToolsChange {
                active_tool_names: next_active.clone(),
            };
            self.enqueue_pending_write(write).await?;
        }

        *self.shared.tools.lock().await = next_tools;
        *self.shared.active_tool_names.lock().await = next_active.clone();
        self.emit_own(AgentHarnessOwnEvent::ToolsUpdate(
            crate::agent::harness::types::ToolsUpdateEvent {
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
            let write = PendingSessionWrite::ActiveToolsChange {
                active_tool_names: tool_names.clone(),
            };
            self.enqueue_pending_write(write).await?;
        }

        *self.shared.active_tool_names.lock().await = tool_names.clone();
        self.emit_own(AgentHarnessOwnEvent::ToolsUpdate(
            crate::agent::harness::types::ToolsUpdateEvent {
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
            crate::agent::harness::types::ResourcesUpdateEvent {
                resources: self.shared.resources.lock().await.clone(),
                previous_resources,
            },
        ))
        .await
    }

    pub async fn set_stream_options(&self, stream_options: AgentHarnessStreamOptions) {
        *self.shared.stream_options.lock().await = clone_stream_options(&stream_options);
    }

    /// Set the TOON prompt-encoding configuration for model-visible tool results.
    /// `None` falls back to `PromptEncodingConfig::from_env()` (`ELPH_PROMPT_ENCODING*` vars).
    pub fn set_prompt_encoding(&self, config: Option<PromptEncodingConfig>) {
        *self.shared.prompt_encoding.lock().unwrap() = config;
    }

    pub async fn set_system_prompt(&self, prompt: SystemPrompt<S>) -> HarnessOpResult<()> {
        *self.shared.system_prompt.lock().await = prompt;
        Ok(())
    }
}

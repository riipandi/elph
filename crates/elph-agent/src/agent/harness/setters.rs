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

    /// Lazily activate tools advertised by a tool result (`added_tool_names`).
    ///
    /// Names that are not present in the harness registry are ignored (they may
    /// belong to a different session or server). Only genuinely new names are
    /// appended — an already-active tool is left untouched. The updated set is
    /// persisted durably the same way [`Self::set_active_tools`] does (pending
    /// write while a turn is active), and emits a `ToolsUpdate` event so guests
    /// (UI, extensions) observe the activation.
    pub(crate) async fn activate_lazy_tools(&self, names: &[String]) -> HarnessOpResult<()> {
        // Snapshot registry then release the tools lock. This method runs from
        // `after_tool_call` (nested under the agent turn); re-locking `tools` while
        // building the ToolsUpdate event can deadlock the async Mutex on some paths.
        let registered = self.shared.tools.lock().await.clone();
        let registered_names: Vec<String> = registered.keys().cloned().collect();
        let existing: Vec<String> = self.shared.active_tool_names.lock().await.clone();
        let fresh: Vec<String> = filter_lazy_names(names, &existing, &registered);
        if fresh.is_empty() {
            let unknown: Vec<&String> = names.iter().filter(|n| !registered.contains_key(*n)).collect();
            if !unknown.is_empty() {
                log::warn!("lazy activation skipped: tools not in registry (catalog may be stale): {unknown:?}");
            }
            return Ok(());
        }

        let mut next = existing;
        next.extend(fresh.clone());

        // Keep baseline in sync so collaboration-mode rewrites (Plan ↔ Default)
        // still see session-activated tools when re-filtering.
        {
            let mut baseline = self.shared.baseline_active_tool_names.lock().await;
            for name in &fresh {
                if !baseline.iter().any(|n| n == name) {
                    baseline.push(name.clone());
                }
            }
        }

        if self.phase_async().await == AgentHarnessPhase::Idle {
            self.shared
                .session
                .lock()
                .await
                .append_active_tools_change(next.clone())
                .await
                .map_err(session_error)?;
        } else {
            let write = PendingSessionWrite::ActiveToolsChange {
                active_tool_names: next.clone(),
            };
            self.enqueue_pending_write(write).await?;
        }

        let previous = self.shared.active_tool_names.lock().await.clone();
        *self.shared.active_tool_names.lock().await = next.clone();
        self.emit_own(AgentHarnessOwnEvent::ToolsUpdate(
            crate::agent::harness::types::ToolsUpdateEvent {
                tool_names: registered_names.clone(),
                previous_tool_names: registered_names,
                active_tool_names: next,
                previous_active_tool_names: previous,
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
    /// `None` falls back to `PromptEncodingConfig::from_env()` (`{PREFIX}_PROMPT_ENCODING*`, default `ELPH`).
    pub fn set_prompt_encoding(&self, config: Option<PromptEncodingConfig>) {
        *self.shared.prompt_encoding.lock().unwrap() = config;
    }

    pub async fn set_system_prompt(&self, prompt: SystemPrompt<S>) -> HarnessOpResult<()> {
        *self.shared.system_prompt.lock().await = prompt;
        Ok(())
    }
}

/// Compute tool names that are genuinely new (registered and not already active).
///
/// Pure logic extracted from [`AgentHarness::activate_lazy_tools`] so the filter
/// behavior is unit-testable without a live harness.
fn filter_lazy_names(
    advertised: &[String],
    existing: &[String],
    registered: &HashMap<String, crate::types::AgentTool>,
) -> Vec<String> {
    advertised
        .iter()
        .filter(|name| registered.contains_key(*name) && !existing.iter().any(|n| n == *name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> crate::types::AgentTool {
        crate::tools::simple_tool(
            elph_ai::Tool {
                name: name.into(),
                constrained_sampling: None,
                description: format!("{name} tool"),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
            name,
            |_, _| Box::pin(async move { Ok(crate::types::AgentToolResult::text("ok")) }),
        )
    }

    #[test]
    fn lazy_filter_adds_only_new_registered_names() {
        let registered: HashMap<String, crate::types::AgentTool> =
            vec![tool("mcp_a__x"), tool("mcp_a__y"), tool("read_file")]
                .into_iter()
                .map(|t| (t.name().to_string(), t))
                .collect();
        let existing = vec!["read_file".to_string()];

        let fresh = filter_lazy_names(
            &[
                "mcp_a__x".into(),
                "mcp_a__y".into(),
                "read_file".into(),
                "missing".into(),
            ],
            &existing,
            &registered,
        );
        let mut fresh = fresh;
        fresh.sort();
        assert_eq!(fresh, vec!["mcp_a__x".to_string(), "mcp_a__y".to_string()]);
    }

    #[test]
    fn lazy_filter_ignores_unknown_and_already_active() {
        let registered: HashMap<String, crate::types::AgentTool> = vec![tool("exists")]
            .into_iter()
            .map(|t| (t.name().to_string(), t))
            .collect();
        let existing = vec!["exists".to_string(), "read_file".to_string()];

        let fresh = filter_lazy_names(&["exists".into(), "read_file".into(), "nope".into()], &existing, &registered);
        assert!(fresh.is_empty());
    }
}

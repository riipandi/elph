//! Extension registry — discovery, load, slash dispatch, harness bind.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde_json::Value;
use wasmi::Engine;

use super::discovery::{discover_manifests, extension_roots};
use super::host::{LoadedExtension, default_ui, new_engine};
use super::types::{ExtensionCommand, ExtensionManifest, ExtensionSlashResult, ExtensionToolSpec, ExtensionsSettings};
use super::ui::ExtensionUi;
use crate::agent::harness::AgentHarness;
use crate::session::types::{HasSessionId, SessionStorage};
use crate::tools::simple_tool;
use crate::tools::types::{AgentTool, AgentToolResult};
use elph_ai::Tool;

struct RegistryState {
    engine: Engine,
    ui: Arc<dyn ExtensionUi>,
    extensions: Vec<Arc<LoadedExtension>>,
    commands: Vec<ExtensionCommand>,
}

pub struct ExtensionRegistry {
    inner: RwLock<RegistryState>,
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        let engine = new_engine().unwrap_or_else(|error| {
            log::error!("wasmi engine: {error:#}");
            Engine::default()
        });
        Self {
            inner: RwLock::new(RegistryState {
                engine,
                ui: default_ui(),
                extensions: Vec::new(),
                commands: Vec::new(),
            }),
        }
    }

    pub fn set_ui(&self, ui: Arc<dyn ExtensionUi>) {
        self.inner.write().ui = ui;
    }

    pub fn load(
        &self,
        config_dir: &Path,
        project_elph_dir: &Path,
        settings: &ExtensionsSettings,
        include_project: bool,
    ) -> Result<()> {
        let roots = extension_roots(config_dir, project_elph_dir, settings, include_project);
        let manifests = discover_manifests(&roots)?;
        let (engine, ui) = {
            let state = self.inner.read();
            (state.engine.clone(), Arc::clone(&state.ui))
        };
        let mut extensions = Vec::new();
        let mut commands = Vec::new();

        for (root, manifest) in manifests {
            if !manifest.enabled || !settings.is_enabled(&manifest.name) {
                log::debug!("extension skipped name={}", manifest.name);
                continue;
            }
            let loaded = LoadedExtension::load(&engine, &root, manifest, Arc::clone(&ui))
                .with_context(|| format!("load extension {}", root.display()))?;
            commands.extend(loaded.commands());
            extensions.push(Arc::new(loaded));
        }

        commands.sort_by(|a, b| a.name.cmp(&b.name));
        log::info!("extensions loaded count={} commands={}", extensions.len(), commands.len());
        let mut state = self.inner.write();
        state.extensions = extensions;
        state.commands = commands;
        Ok(())
    }

    pub fn commands(&self) -> Vec<ExtensionCommand> {
        self.inner.read().commands.clone()
    }

    pub fn extensions(&self) -> Vec<ExtensionManifest> {
        self.inner
            .read()
            .extensions
            .iter()
            .map(|ext| ext.manifest.clone())
            .collect()
    }

    pub fn tool_specs(&self) -> Vec<(String, ExtensionToolSpec)> {
        self.inner
            .read()
            .extensions
            .iter()
            .flat_map(|ext| ext.tools().into_iter().map(|spec| (ext.manifest.name.clone(), spec)))
            .collect()
    }

    pub fn dispatch_slash(&self, name: &str, args: &str) -> Option<Result<ExtensionSlashResult>> {
        let state = self.inner.read();
        let owner = state.commands.iter().find(|cmd| cmd.name.eq_ignore_ascii_case(name))?;
        let extension = state
            .extensions
            .iter()
            .find(|ext| ext.manifest.name == owner.extension)?;
        let extension = Arc::clone(extension);
        let command = owner.name.clone();
        drop(state);
        Some(
            extension
                .execute_command(&command, args)
                .with_context(|| format!("extension /{name}")),
        )
    }

    pub fn dispatch_event(&self, event: &str, payload: &Value) -> Option<Value> {
        let extensions: Vec<Arc<LoadedExtension>> = self.inner.read().extensions.clone();
        let mut last = None;
        for ext in extensions {
            if !ext.subscribed(event) {
                continue;
            }
            match ext.on_event(event, payload) {
                Ok(Some(value)) => last = Some(value),
                Ok(None) => {}
                Err(error) => log::warn!("extension event {event} failed: {error:#}"),
            }
        }
        last
    }

    pub fn execute_tool(&self, extension: &str, name: &str, tool_call_id: &str, input: &Value) -> Result<Value> {
        let loaded = {
            let state = self.inner.read();
            state
                .extensions
                .iter()
                .find(|ext| ext.manifest.name == extension)
                .cloned()
                .with_context(|| format!("extension not loaded: {extension}"))?
        };
        loaded.execute_tool(name, tool_call_id, input)
    }

    pub fn agent_tools(&self) -> Vec<AgentTool> {
        let loaded = self.inner.read().extensions.clone();
        let mut tools = Vec::new();
        for ext in loaded {
            for spec in ext.tools() {
                let label = if spec.label.is_empty() {
                    spec.name.clone()
                } else {
                    spec.label.clone()
                };
                let parameters = if spec.parameters.is_null() {
                    serde_json::json!({ "type": "object", "properties": {} })
                } else {
                    spec.parameters.clone()
                };
                let tool_name = spec.name.clone();
                let ext = Arc::clone(&ext);
                tools.push(simple_tool(
                    Tool {
                        name: spec.name.clone(),
                        constrained_sampling: None,
                        description: spec.description,
                        parameters,
                    },
                    label,
                    move |id, args| {
                        let ext = Arc::clone(&ext);
                        let tool_name = tool_name.clone();
                        Box::pin(async move {
                            let value = tokio::task::spawn_blocking(move || ext.execute_tool(&tool_name, &id, &args))
                                .await
                                .map_err(|error| anyhow::anyhow!("join: {error}"))??;
                            Ok(tool_result_from_json(value))
                        })
                    },
                ));
            }
        }
        tools
    }

    pub async fn bind_to_harness<S>(&self, harness: &AgentHarness<S>)
    where
        S: SessionStorage + Clone + Send + Sync + 'static,
        S::Metadata: HasSessionId + Send + Sync,
    {
        let extra = self.agent_tools();
        if !extra.is_empty() {
            let mut tools = harness.get_tools().await;
            let mut active = harness
                .get_active_tools()
                .await
                .into_iter()
                .map(|t| t.name().to_string())
                .collect::<Vec<_>>();
            for tool in extra {
                if !active.iter().any(|n| n == tool.name()) {
                    active.push(tool.name().to_string());
                }
                tools.push(tool);
            }
            if let Err(error) = harness.set_tools(tools, Some(active)).await {
                log::warn!("extension tools: {error:#}");
            }
        }

        let guests = self.inner.read().extensions.clone();

        harness
            .on_tool_call({
                let guests = guests.clone();
                move |event| {
                    let guests = guests.clone();
                    let payload = serde_json::json!({
                        "tool_name": event.tool_name,
                        "tool_call_id": event.tool_call_id,
                        "input": event.input,
                    });
                    async move {
                        let result = tokio::task::spawn_blocking(move || fanout_event(&guests, "tool_call", &payload))
                            .await
                            .ok()
                            .flatten();
                        result.and_then(|value| {
                            let block = value.get("block").and_then(Value::as_bool).unwrap_or(false);
                            if !block {
                                return None;
                            }
                            Some(crate::agent::harness::types::ToolCallHookResult {
                                block: true,
                                reason: value.get("reason").and_then(Value::as_str).map(str::to_string),
                            })
                        })
                    }
                }
            })
            .await;

        harness
            .on_tool_result({
                let guests = guests.clone();
                move |event| {
                    let guests = guests.clone();
                    let payload = serde_json::json!({
                        "tool_name": event.tool_name,
                        "tool_call_id": event.tool_call_id,
                        "input": event.input,
                        "is_error": event.is_error,
                    });
                    async move {
                        let result =
                            tokio::task::spawn_blocking(move || fanout_event(&guests, "tool_result", &payload))
                                .await
                                .ok()
                                .flatten();
                        result.map(|value| crate::agent::harness::types::ToolResultPatch {
                            is_error: value.get("is_error").and_then(Value::as_bool),
                            ..Default::default()
                        })
                    }
                }
            })
            .await;

        harness
            .on_before_agent_start({
                let guests = guests.clone();
                move |event| {
                    let guests = guests.clone();
                    let payload = serde_json::json!({
                        "prompt": event.prompt,
                        "system_prompt": event.system_prompt,
                    });
                    async move {
                        let result =
                            tokio::task::spawn_blocking(move || fanout_event(&guests, "before_agent_start", &payload))
                                .await
                                .ok()
                                .flatten();
                        result.map(|value| crate::agent::harness::types::BeforeAgentStartResult {
                            messages: None,
                            system_prompt: value.get("system_prompt").and_then(Value::as_str).map(str::to_string),
                        })
                    }
                }
            })
            .await;

        let _ = self.dispatch_event("session_start", &serde_json::json!({}));
    }

    pub fn install_bundle(&self, source_dir: &Path, config_dir: &Path, force: bool) -> Result<PathBuf> {
        let manifest_path = source_dir.join("extension.toml");
        let manifest = super::discovery::load_manifest(&manifest_path)?;
        let dest = config_dir.join("extensions").join(&manifest.name);
        if dest.exists() && !force {
            anyhow::bail!("extension '{}' already installed at {}", manifest.name, dest.display());
        }
        std::fs::create_dir_all(config_dir.join("extensions")).context("create extensions dir")?;
        if dest.exists() {
            std::fs::remove_dir_all(&dest).with_context(|| format!("remove {}", dest.display()))?;
        }
        copy_dir_recursive(source_dir, &dest)?;
        Ok(dest)
    }
}

fn fanout_event(extensions: &[Arc<LoadedExtension>], event: &str, payload: &Value) -> Option<Value> {
    let mut last = None;
    for ext in extensions {
        if !ext.subscribed(event) {
            continue;
        }
        match ext.on_event(event, payload) {
            Ok(Some(value)) => last = Some(value),
            Ok(None) => {}
            Err(error) => log::warn!("extension event {event} failed: {error:#}"),
        }
    }
    last
}

fn tool_result_from_json(value: Value) -> AgentToolResult {
    if let Some(message) = value.get("message").and_then(Value::as_str) {
        return AgentToolResult::text(message);
    }
    if let Some(text) = value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
    {
        return AgentToolResult::text(text);
    }
    AgentToolResult::text(value.to_string())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("read dir {}", src.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("copy {} to {}", entry.path().display(), target.display()))?;
        }
    }
    Ok(())
}

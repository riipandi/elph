//! Native lifecycle hooks configured by `hooks.json`.
//!
//! The `ELPH_SESSION_ID` env var passed to hook children is read from the
//! parent process environment. `bind_to_harness` stores the runtime session id
//! on `HookHost` so callers that run hooks after `bind_to_harness` on a
//! different thread can still resolve it.

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use elph_agent::harness::{
    AgentHarness, BeforeAgentStartEvent, SessionBeforeCompactEvent, ToolCallEvent, ToolResultEvent,
};
use elph_agent::session::types::{HasSessionId, SessionStorage};
use elph_agent::types::{AgentMessage, ToolResultContent};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

const HOOK_SCHEMA_JSON: &str = include_str!("../../../../../schemas/hooks-schema.json");
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const MAX_TIMEOUT_MS: u64 = 60_000;
const MAX_INPUT_BYTES: usize = 128 * 1024;
const MAX_OUTPUT_BYTES: u64 = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookConfig {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub hooks: Vec<HookDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookDefinition {
    pub id: String,
    pub event: HookEvent,
    #[serde(default)]
    pub matcher: Option<HookMatcher>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(skip)]
    pub source: PathBuf,
    #[serde(skip)]
    pub working_dir: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookMatcher {
    #[serde(default)]
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HookEvent {
    SessionStart,
    UserPromptSubmit,
    BeforeAgent,
    Context,
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PreCompact,
    PostCompact,
    Stop,
    SessionEnd,
}

#[derive(Debug, Clone, Default)]
pub struct HookStatus {
    pub active: Vec<HookDefinition>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Default)]
pub struct HookHost {
    config: Arc<RwLock<HookStatus>>,
    session_id: Arc<RwLock<Option<String>>>,
}

impl std::fmt::Debug for HookHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookHost").finish_non_exhaustive()
    }
}

impl HookHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> HookStatus {
        self.config.read().clone()
    }

    pub fn reload(&self, paths: &crate::platform::Paths) -> Result<()> {
        let mut hooks = Vec::new();
        let mut diagnostics = Vec::new();
        for (is_project, path) in [
            (false, paths.global_hooks_config_path()),
            (true, paths.project_hooks_config_path()),
        ] {
            if is_project
                && !crate::platform::scaffold::TrustStore::project_hooks_allowed(paths, paths.project_dir())
                    .unwrap_or(false)
            {
                if path.is_file() {
                    diagnostics.push(format!(
                        "project hooks skipped until {} is trusted",
                        paths.project_dir().display()
                    ));
                }
                continue;
            }
            match load_file(&path, paths.project_dir()) {
                Ok(mut loaded) => hooks.append(&mut loaded),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
                Err(error) => diagnostics.push(format!("{}: {error:#}", path.display())),
            }
        }

        let mut ids = HashSet::new();
        hooks.retain(|hook| {
            if !ids.insert(hook.id.clone()) {
                diagnostics.push(format!("duplicate hook id '{}'", hook.id));
                false
            } else {
                true
            }
        });
        *self.config.write() = HookStatus {
            active: hooks.into_iter().filter(|hook| hook.enabled).collect(),
            diagnostics,
        };
        Ok(())
    }

    pub async fn bind_to_harness<S>(&self, harness: &AgentHarness<S>)
    where
        S: SessionStorage + Clone + Send + Sync + 'static,
        S::Metadata: HasSessionId + Send + Sync,
    {
        let session_id = harness.session_metadata().await.session_id().to_string();
        *self.session_id.write() = Some(session_id.clone());
        for hook in self.handlers(HookEvent::SessionStart) {
            deliver_session_start(harness, &hook).await;
        }

        let session_id_for_hooks = session_id.clone();
        if !self.handlers(HookEvent::UserPromptSubmit).is_empty() || !self.handlers(HookEvent::BeforeAgent).is_empty() {
            let host = self.clone();
            let value = session_id_for_hooks.clone();
            harness
                .on_before_agent_start(move |event: &BeforeAgentStartEvent| {
                    let user_prompt_submit = host.handlers(HookEvent::UserPromptSubmit);
                    let handlers = host.handlers(HookEvent::BeforeAgent);
                    let event = serde_json::to_value(BeforeAgentPayload::from(event)).unwrap_or_default();
                    let session_id = value.clone();
                    Box::pin(async move {
                        let user_prompt_event = serde_json::json!({
                            "event": "userPromptSubmit",
                            "payload": event.clone(),
                        });
                        for hook in user_prompt_submit {
                            let _ = execute_hook_with_session(&hook, &user_prompt_event, Some(&session_id)).await;
                        }
                        let mut system_prompt = None;
                        let mut messages = Vec::new();
                        for hook in handlers {
                            if let Some(output) = execute_hook_with_session(&hook, &event, Some(&session_id)).await {
                                if let Some(prompt) = bounded_string(output.get("systemPrompt")) {
                                    append_prompt_fragment(
                                        system_prompt.get_or_insert_with(|| {
                                            event["systemPrompt"].as_str().unwrap_or_default().to_string()
                                        }),
                                        &prompt,
                                    );
                                }
                                if let Some(context) = bounded_string(output.get("additionalContext")) {
                                    append_prompt_fragment(
                                        system_prompt.get_or_insert_with(|| {
                                            event["systemPrompt"].as_str().unwrap_or_default().to_string()
                                        }),
                                        &context,
                                    );
                                }
                                if let Some(extra) = hook_messages(&output, &hook.id) {
                                    messages.extend(extra);
                                }
                            }
                        }
                        (system_prompt.is_some() || !messages.is_empty()).then_some(
                            elph_agent::harness::BeforeAgentStartResult {
                                system_prompt,
                                messages: (!messages.is_empty()).then_some(messages),
                            },
                        )
                    })
                })
                .await;
        }

        if !self.handlers(HookEvent::Context).is_empty() {
            let host = self.clone();
            let value = session_id_for_hooks.clone();
            harness
                .on_context(move |event| {
                    let handlers = host.handlers(HookEvent::Context);
                    let original_messages = event.messages.clone();
                    let session_id = value.clone();
                    Box::pin(async move {
                        let mut messages = original_messages;
                        let mut changed = false;
                        for hook in handlers {
                            let payload = serde_json::json!({
                                "event": "context",
                                "messages": messages.clone(),
                            });
                            if let Some(output) = execute_hook_with_session(&hook, &payload, Some(&session_id)).await
                                && let Some(replacement) = hook_messages(&output, &hook.id)
                            {
                                messages = replacement;
                                changed = true;
                            }
                        }
                        Ok(changed.then_some(elph_agent::harness::ContextResult { messages }))
                    })
                })
                .await;
        }

        if !self.handlers(HookEvent::PreToolUse).is_empty() {
            let host = self.clone();
            let value = session_id_for_hooks.clone();
            harness
                .on_tool_call(move |event: &ToolCallEvent| {
                    let handlers = host.handlers(HookEvent::PreToolUse);
                    let tool_name = event.tool_name.clone();
                    let tool_call_id = event.tool_call_id.clone();
                    let original_input = event.input.clone();
                    let session_id = value.clone();
                    Box::pin(async move {
                        let mut input = original_input.clone();
                        let mut blocked = None;
                        for hook in handlers {
                            if !matches_tool(hook.matcher.as_ref(), &tool_name) {
                                continue;
                            }
                            let payload = serde_json::to_value(ToolCallPayload {
                                tool_name: &tool_name,
                                tool_call_id: &tool_call_id,
                                tool_input: &input,
                            })
                            .unwrap_or_default();
                            if let Some(output) = execute_hook_with_session(&hook, &payload, Some(&session_id)).await {
                                if let Some(replacement) = output.get("toolInput") {
                                    input = replacement.clone();
                                }
                                let is_blocked = output
                                    .get("decision")
                                    .and_then(Value::as_str)
                                    .is_some_and(|decision| decision == "deny")
                                    || output.get("block").and_then(Value::as_bool).unwrap_or(false);
                                if is_blocked {
                                    blocked = Some(bounded_string(output.get("reason")));
                                    break;
                                }
                            }
                        }
                        blocked
                            .map(|reason| elph_agent::harness::ToolCallHookResult {
                                block: true,
                                reason,
                                args: None,
                            })
                            .or_else(|| {
                                (input != original_input).then_some(elph_agent::harness::ToolCallHookResult {
                                    block: false,
                                    reason: None,
                                    args: Some(input),
                                })
                            })
                    })
                })
                .await;
        }

        if !self.handlers(HookEvent::PostToolUse).is_empty() || !self.handlers(HookEvent::PostToolUseFailure).is_empty()
        {
            let host = self.clone();
            let value = session_id_for_hooks.clone();
            harness
                .on_tool_result(move |event: &ToolResultEvent| {
                    let post_tool = host.handlers(HookEvent::PostToolUse);
                    let post_tool_failure = host.handlers(HookEvent::PostToolUseFailure);
                    let handlers = if event.is_error { post_tool_failure } else { post_tool };
                    let mut payload = serde_json::to_value(ToolResultPayload::from(event)).unwrap_or_default();
                    let session_id = value.clone();
                    Box::pin(async move {
                        let mut patch = elph_agent::harness::ToolResultPatch::default();
                        let mut changed = false;
                        for hook in handlers {
                            if !matches_tool(hook.matcher.as_ref(), payload["toolName"].as_str().unwrap_or_default()) {
                                continue;
                            }
                            if let Some(output) = execute_hook_with_session(&hook, &payload, Some(&session_id)).await {
                                if let Some(content) = output.get("content")
                                    && let Some(content) = hook_tool_result_content(content, &hook.id)
                                {
                                    payload["content"] = serde_json::to_value(&content).unwrap_or_default();
                                    patch.content = Some(content);
                                    changed = true;
                                }
                                if let Some(value) = output.get("isError").and_then(Value::as_bool) {
                                    payload["isError"] = Value::Bool(value);
                                    patch.is_error = Some(value);
                                    changed = true;
                                }
                                if let Some(value) = output.get("details") {
                                    payload["details"] = value.clone();
                                    patch.details = Some(value.clone());
                                    changed = true;
                                }
                                if let Some(value) = output.get("terminate").and_then(Value::as_bool) {
                                    patch.terminate = Some(value);
                                    changed = true;
                                }
                            }
                        }
                        changed.then_some(patch)
                    })
                })
                .await;
        }

        if !self.handlers(HookEvent::PreCompact).is_empty() {
            let host = self.clone();
            let value = session_id_for_hooks.clone();
            harness
                .on_session_before_compact(move |event: &SessionBeforeCompactEvent| {
                    let handlers = host.handlers(HookEvent::PreCompact);
                    let payload = serde_json::to_value(CompactPayload::from(event)).unwrap_or_default();
                    let session_id = value.clone();
                    Box::pin(async move {
                        let mut result = elph_agent::harness::SessionBeforeCompactResult::default();
                        let mut changed = false;
                        for hook in handlers {
                            if let Some(output) = execute_hook_with_session(&hook, &payload, Some(&session_id)).await {
                                if output.get("cancel").and_then(Value::as_bool).unwrap_or(false) {
                                    result.cancel = true;
                                    changed = true;
                                }
                                if let Some(instructions) = bounded_string(output.get("customInstructions")) {
                                    result.custom_instructions = Some(instructions);
                                    changed = true;
                                }
                            }
                        }
                        changed.then_some(result)
                    })
                })
                .await;
        }

        if !self.handlers(HookEvent::PostCompact).is_empty() || !self.handlers(HookEvent::Stop).is_empty() {
            let host = self.clone();
            let session_id = session_id_for_hooks.clone();
            harness
                .subscribe(move |event, _signal| -> Pin<Box<dyn Future<Output = ()> + Send>> {
                    let session_id = session_id.clone();
                    let (handlers, payload) = match event {
                        elph_agent::harness::AgentHarnessEvent::Own(
                            elph_agent::harness::AgentHarnessOwnEvent::SessionCompact(_),
                        ) => (
                            host.handlers(HookEvent::PostCompact),
                            serde_json::json!({"event": "postCompact"}),
                        ),
                        elph_agent::harness::AgentHarnessEvent::Own(
                            elph_agent::harness::AgentHarnessOwnEvent::Settled(settled),
                        ) => (
                            host.handlers(HookEvent::Stop),
                            serde_json::json!({
                                "event": "stop",
                                "nextTurnCount": settled.next_turn_count,
                            }),
                        ),
                        _ => return Box::pin(async {}),
                    };
                    Box::pin(async move {
                        for hook in handlers {
                            let _ = execute_hook_with_session(&hook, &payload, Some(&session_id)).await;
                        }
                    })
                })
                .await;
        }
    }

    fn handlers(&self, event: HookEvent) -> Vec<HookDefinition> {
        self.config
            .read()
            .active
            .iter()
            .filter(|hook| hook.event == event)
            .cloned()
            .collect()
    }
}

async fn deliver_session_start<S>(harness: &AgentHarness<S>, hook: &HookDefinition)
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    let session_id = harness.session_metadata().await.session_id().to_string();
    let payload = serde_json::json!({
        "event": "sessionStart",
        "sessionId": session_id,
    });
    let _ = execute_hook_with_session(hook, &payload, Some(&session_id)).await;
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_true() -> bool {
    true
}

fn load_file(path: &Path, cwd: &Path) -> Result<Vec<HookDefinition>> {
    let bytes = std::fs::read(path).with_context(|| format!("read hook config {}", path.display()))?;
    let value: Value =
        serde_json::from_slice(&bytes).with_context(|| format!("parse hook config {}", path.display()))?;
    jsonschema::validate(&serde_json::from_str(HOOK_SCHEMA_JSON)?, &value)
        .map_err(|error| anyhow::anyhow!("schema validation: {error}"))?;
    let mut config: HookConfig =
        serde_json::from_value(value).with_context(|| format!("decode hook config {}", path.display()))?;
    for hook in &mut config.hooks {
        if hook.timeout_ms == 0 || hook.timeout_ms > MAX_TIMEOUT_MS {
            bail!("hook '{}' timeoutMs must be between 1 and {MAX_TIMEOUT_MS}", hook.id);
        }
        hook.source = path.to_path_buf();
        hook.working_dir = cwd.to_path_buf();
    }
    Ok(config.hooks)
}

fn matches_tool(matcher: Option<&HookMatcher>, tool_name: &str) -> bool {
    matcher.is_none_or(|matcher| {
        matcher.tool_names.is_empty() || matcher.tool_names.iter().any(|pattern| wildcard(pattern, tool_name))
    })
}

fn wildcard(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        true
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        value.starts_with(prefix)
    } else if let Some(suffix) = pattern.strip_prefix('*') {
        value.ends_with(suffix)
    } else {
        pattern == value
    }
}

async fn execute_hook_with_session(hook: &HookDefinition, payload: &Value, session_id: Option<&str>) -> Option<Value> {
    let mut input = match serde_json::to_vec(payload) {
        Ok(input) if input.len() <= MAX_INPUT_BYTES => input,
        Ok(_) => {
            log::warn!("hook {} skipped: input exceeds {} bytes", hook.id, MAX_INPUT_BYTES);
            return None;
        }
        Err(error) => {
            log::warn!("hook {} input serialization failed: {error}", hook.id);
            return None;
        }
    };
    input.push(b'\n');

    let command_path = Path::new(&hook.command);
    let command = if command_path.is_relative() && command_path.components().count() > 1 {
        hook.source
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&hook.command)
    } else {
        PathBuf::from(&hook.command)
    };
    let mut process = Command::new(command);
    process.args(&hook.args).current_dir(&hook.working_dir);
    process.env_clear();
    for name in [
        "HOME",
        "PATH",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USERPROFILE",
        "SystemRoot",
        "WINDIR",
    ] {
        if let Some(value) = std::env::var_os(name) {
            process.env(name, value);
        }
    }
    process.env("ELPH_HOOK_ID", &hook.id);
    for (name, value) in hook_env(session_id) {
        if let Some(value) = value {
            process.env(name, value);
        }
    }
    for name in ["HERDR_ENV", "HERDR_PANE_ID", "HERDR_SOCKET_PATH", "HERDR_BIN_PATH"] {
        if let Some(value) = std::env::var_os(name) {
            process.env(name, value);
        }
    }
    process.stdin(std::process::Stdio::piped());
    process.stdout(std::process::Stdio::piped());
    process.stderr(std::process::Stdio::piped());
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            log::warn!("hook {} failed to spawn: {error}", hook.id);
            return None;
        }
    };
    let mut stdin = child.stdin.take()?;
    if stdin.write_all(&input).await.is_err() {
        let _ = child.kill().await;
        return None;
    }
    drop(stdin);
    let stdout = child.stdout.take()?;
    let stderr = child.stderr.take()?;
    let run = async {
        let stdout = read_limited(stdout, MAX_OUTPUT_BYTES);
        let stderr = read_limited(stderr, MAX_OUTPUT_BYTES);
        let (stdout, stderr, status) = tokio::join!(stdout, stderr, child.wait());
        (stdout, stderr, status)
    };
    let result = match tokio::time::timeout(Duration::from_millis(hook.timeout_ms), run).await {
        Ok((Ok(stdout), Ok(stderr), Ok(status))) if status.success() => (stdout, stderr),
        Ok((stdout, stderr, status)) => {
            log::warn!(
                "hook {} failed: status={:?} stdout_bytes={} stderr_bytes={}",
                hook.id,
                status,
                output_size(&stdout),
                output_size(&stderr),
            );
            return None;
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            log::warn!("hook {} timed out after {}ms", hook.id, hook.timeout_ms);
            return None;
        }
    };
    if result.0.len() as u64 > MAX_OUTPUT_BYTES {
        log::warn!("hook {} stdout exceeds {} bytes", hook.id, MAX_OUTPUT_BYTES);
        return None;
    }
    if result.1.len() as u64 > MAX_OUTPUT_BYTES {
        log::warn!("hook {} stderr exceeds {} bytes", hook.id, MAX_OUTPUT_BYTES);
        return None;
    }
    if result.0.iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    match serde_json::from_slice(&result.0) {
        Ok(value) => Some(value),
        Err(error) => {
            log::warn!("hook {} returned invalid JSON: {error}", hook.id);
            None
        }
    }
}

async fn read_limited<R: AsyncRead + Unpin>(reader: R, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes).await?;
    Ok(bytes)
}

fn output_size(output: &std::io::Result<Vec<u8>>) -> usize {
    output.as_ref().map_or(0, Vec::len)
}

fn bounded_string(value: Option<&Value>) -> Option<String> {
    let value = value?.as_str()?;
    (value.len() <= MAX_CONTEXT_BYTES).then(|| value.to_string())
}

fn hook_env(session_id: Option<&str>) -> Vec<(&'static str, Option<String>)> {
    // Runtime session id from `HookHost` wins; fall back to a caller-provided
    // variable (workers, headless child processes) when present.
    let session_id = session_id
        .map(str::to_string)
        .or_else(|| std::env::var("ELPH_SESSION_ID").ok())
        .filter(|id| !id.is_empty());
    vec![
        ("ELPH_SESSION_ID", session_id),
        ("ELPH_PROJECT_DIR", std::env::var("ELPH_PROJECT_DIR").ok()),
    ]
}

fn append_prompt_fragment(prompt: &mut String, fragment: &str) {
    if !prompt.is_empty() {
        prompt.push_str("\n\n");
    }
    prompt.push_str(fragment);
}

#[cfg(test)]
mod env_tests {
    use super::*;

    #[test]
    fn hook_env_forwards_elph_identity_vars() {
        let names: Vec<&str> = hook_env(None).into_iter().map(|(name, _)| name).collect();
        for name in ["ELPH_SESSION_ID", "ELPH_PROJECT_DIR"] {
            assert!(names.contains(&name), "hook must forward {name}");
        }
    }

    #[test]
    fn hook_env_prefers_runtime_session_id() {
        unsafe {
            std::env::set_var("ELPH_SESSION_ID", "env-session");
        }
        let env = hook_env(Some("runtime-session"));
        assert_eq!(env[0].1.as_deref(), Some("runtime-session"), "runtime wins");
        unsafe {
            std::env::remove_var("ELPH_SESSION_ID");
        }
    }
}

fn hook_messages(output: &Value, hook_id: &str) -> Option<Vec<AgentMessage>> {
    let value = output.get("messages")?;
    match serde_json::from_value(value.clone()) {
        Ok(messages) => Some(messages),
        Err(error) => {
            log::warn!("hook {hook_id} returned invalid messages: {error}");
            None
        }
    }
}

fn hook_tool_result_content(output: &Value, hook_id: &str) -> Option<Vec<ToolResultContent>> {
    let content: Vec<ToolResultContent> = match serde_json::from_value(output.clone()) {
        Ok(content) => content,
        Err(error) => {
            log::warn!("hook {hook_id} returned invalid tool result content: {error}");
            return None;
        }
    };
    if content.iter().all(|block| match block {
        ToolResultContent::Text(text) => text.kind == "text",
        ToolResultContent::Image(image) => image.kind == "image",
    }) {
        Some(content)
    } else {
        log::warn!("hook {hook_id} returned an unsupported tool result content type");
        None
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BeforeAgentPayload<'a> {
    prompt: &'a str,
    system_prompt: &'a str,
}

impl<'a> From<&'a BeforeAgentStartEvent> for BeforeAgentPayload<'a> {
    fn from(event: &'a BeforeAgentStartEvent) -> Self {
        Self {
            prompt: &event.prompt,
            system_prompt: &event.system_prompt,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallPayload<'a> {
    tool_name: &'a str,
    tool_call_id: &'a str,
    tool_input: &'a Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolResultPayload<'a> {
    tool_name: &'a str,
    tool_call_id: &'a str,
    tool_input: &'a Value,
    content: &'a [ToolResultContent],
    details: &'a Value,
    is_error: bool,
}

impl<'a> From<&'a ToolResultEvent> for ToolResultPayload<'a> {
    fn from(event: &'a ToolResultEvent) -> Self {
        Self {
            tool_name: &event.tool_name,
            tool_call_id: &event.tool_call_id,
            tool_input: &event.input,
            content: &event.content,
            details: &event.details,
            is_error: event.is_error,
        }
    }
}

#[derive(Serialize)]
struct CompactPayload {
    branch_entry_count: usize,
    custom_instructions: Option<String>,
}

impl From<&SessionBeforeCompactEvent> for CompactPayload {
    fn from(event: &SessionBeforeCompactEvent) -> Self {
        Self {
            branch_entry_count: event.branch_entries.len(),
            custom_instructions: event.custom_instructions.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::path::AppPaths;

    #[test]
    fn schema_accepts_direct_command_hook() {
        let value = serde_json::json!({
            "$schema": "https://elph.space/hooks-schema.json",
            "hooks": [{
                "id": "audit",
                "event": "preToolUse",
                "matcher": {"toolNames": ["write_file", "apply_*"]},
                "command": "./audit",
                "timeoutMs": 5000
            }]
        });
        let schema: Value = serde_json::from_str(HOOK_SCHEMA_JSON).expect("schema");
        assert!(jsonschema::validate(&schema, &value).is_ok());
        let config: HookConfig = serde_json::from_value(value).expect("config");
        assert_eq!(config.hooks[0].event, HookEvent::PreToolUse);
    }

    #[test]
    fn schema_accepts_context_hooks() {
        let value = serde_json::json!({
            "hooks": [{
                "id": "trim-context",
                "event": "context",
                "command": "trim-context"
            }]
        });
        let schema: Value = serde_json::from_str(HOOK_SCHEMA_JSON).expect("schema");
        assert!(jsonschema::validate(&schema, &value).is_ok());
        let config: HookConfig = serde_json::from_value(value).expect("config");
        assert_eq!(config.hooks[0].event, HookEvent::Context);
    }

    #[test]
    fn schema_rejects_unknown_fields_and_events() {
        let schema: Value = serde_json::from_str(HOOK_SCHEMA_JSON).expect("schema");
        for value in [
            serde_json::json!({"hooks": [{"id": "x", "event": "unknown", "command": "x"}]}),
            serde_json::json!({"hooks": [{"id": "x", "event": "preToolUse", "command": "x", "shell": true}]}),
            serde_json::json!({"hooks": [{"id": "x", "event": "beforeAgent", "matcher": {"toolNames": ["*"]}, "command": "x"}]}),
        ] {
            assert!(jsonschema::validate(&schema, &value).is_err());
        }
    }

    #[test]
    fn wildcard_matcher_supports_exact_prefix_and_suffix() {
        assert!(wildcard("write_file", "write_file"));
        assert!(wildcard("write_*", "write_file"));
        assert!(wildcard("*file", "write_file"));
        assert!(!wildcard("write_*", "read_file"));
    }

    #[test]
    fn prompt_fragments_are_appended_with_boundaries() {
        let mut prompt = "base".to_string();
        append_prompt_fragment(&mut prompt, "first");
        append_prompt_fragment(&mut prompt, "second");
        assert_eq!(prompt, "base\n\nfirst\n\nsecond");
    }

    #[test]
    fn tool_result_content_accepts_text_and_image_blocks() {
        let content = hook_tool_result_content(
            &serde_json::json!([
                {"type": "text", "text": "replacement"},
                {"type": "image", "data": "AA==", "mime_type": "image/png"}
            ]),
            "replace-result",
        )
        .expect("content");
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn reload_loads_global_hooks_and_reports_duplicate_ids() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::platform::Paths::from_dirs(
            temp.path().join("config"),
            temp.path().join("data"),
            temp.path().join("project"),
        );
        std::fs::create_dir_all(paths.config_dir()).expect("config dir");
        std::fs::write(
            paths.global_hooks_config_path(),
            serde_json::json!({
                "hooks": [
                    {"id": "audit", "event": "sessionStart", "command": "audit"},
                    {"id": "audit", "event": "stop", "command": "audit"}
                ]
            })
            .to_string(),
        )
        .expect("hooks");

        let host = HookHost::new();
        host.reload(&paths).expect("reload");
        let status = host.status();
        assert_eq!(status.active.len(), 1);
        assert_eq!(status.diagnostics, vec!["duplicate hook id 'audit'"]);
        assert_eq!(status.active[0].event, HookEvent::SessionStart);
    }
}

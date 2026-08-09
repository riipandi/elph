//! Agent tools for multi-worker coordination.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

use super::mailbox::MailboxStore;
use super::registry::WorkerRegistry;
use super::types::MessageStatus;

/// Shared context for worker tools (host fills after session start).
#[derive(Clone)]
pub struct WorkerToolContext {
    pub registry: Arc<WorkerRegistry>,
    pub mailbox: Arc<MailboxStore>,
    pub worker_id: String,
    pub session_id: String,
    pub project_key: String,
    pub stale_secs: u64,
    pub ask_timeout_ms: u64,
    pub max_hops: i64,
}

pub fn create_worker_tools(ctx: Arc<WorkerToolContext>) -> Vec<AgentTool> {
    vec![
        worker_list_tool(ctx.clone()),
        worker_send_tool(ctx.clone()),
        worker_get_tool(ctx.clone()),
        worker_await_tool(ctx.clone()),
        worker_ask_tool(ctx),
    ]
}

fn worker_list_tool(ctx: Arc<WorkerToolContext>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "worker_list".into(),
            constrained_sampling: None,
            description: "List live peer Elph workers in this project (other processes).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "include_self": {
                        "type": "boolean",
                        "description": "Include this worker in the list (default false)"
                    }
                }
            }),
        },
        "List workers",
        move |_, args| {
            let ctx = ctx.clone();
            Box::pin(async move {
                let include_self = args.get("include_self").and_then(|v| v.as_bool()).unwrap_or(false);
                let peers = ctx
                    .registry
                    .list_live_peers(&ctx.project_key, &ctx.worker_id, ctx.stale_secs)
                    .await?;
                let list: Vec<_> = peers
                    .into_iter()
                    .filter(|p| include_self || !p.is_self)
                    .collect();
                Ok(AgentToolResult::text(serde_json::to_string_pretty(&list)?))
            })
        },
    )
}

fn worker_send_tool(ctx: Arc<WorkerToolContext>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "worker_send".into(),
            constrained_sampling: None,
            description: "Send a fire-and-forget message to another worker by name or session id. \
                 Do NOT use this to reply to an inbound worker message — answer in normal assistant text."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Peer worker name or session id"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message text to deliver"
                    },
                    "hops": {
                        "type": "integer",
                        "description": "Hop count when forwarding (default 0)"
                    }
                },
                "required": ["target", "message"]
            }),
        },
        "Send to worker",
        move |_, args| {
            let ctx = ctx.clone();
            Box::pin(async move {
                let target = args.get("target").and_then(|v| v.as_str()).unwrap_or("").trim();
                let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("").trim();
                if target.is_empty() || message.is_empty() {
                    bail!("target and message are required");
                }
                let hops = args.get("hops").and_then(|v| v.as_i64()).unwrap_or(0);
                if hops >= ctx.max_hops {
                    bail!("hop limit reached ({hops} >= {})", ctx.max_hops);
                }
                let peer = resolve_peer(&ctx, target).await?;
                let msg = ctx
                    .mailbox
                    .send_prompt(
                        &ctx.project_key,
                        &ctx.worker_id,
                        &ctx.session_id,
                        &peer.session_id,
                        Some(&peer.worker_id),
                        message,
                        hops,
                        None,
                        None,
                    )
                    .await?;
                Ok(AgentToolResult::text(format!(
                    "worker_send → {}\nmsg_id {}\nstatus {}",
                    peer.name,
                    msg.id,
                    msg.status.as_str()
                )))
            })
        },
    )
}

fn worker_get_tool(ctx: Arc<WorkerToolContext>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "worker_get".into(),
            constrained_sampling: None,
            description: "Non-blocking poll of a worker_send/worker_ask message by msg_id.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "msg_id": { "type": "string" }
                },
                "required": ["msg_id"]
            }),
        },
        "Get worker message",
        move |_, args| {
            let ctx = ctx.clone();
            Box::pin(async move {
                let msg_id = args.get("msg_id").and_then(|v| v.as_str()).unwrap_or("");
                let Some(msg) = ctx.mailbox.get(msg_id).await? else {
                    bail!("unknown msg_id {msg_id}");
                };
                if let Some(resp) = ctx.mailbox.get_response_for(msg_id).await? {
                    let text = extract_text(&resp.payload);
                    return Ok(AgentToolResult::text(format!(
                        "status complete\n{}",
                        text.unwrap_or_else(|| resp.payload.clone())
                    )));
                }
                Ok(AgentToolResult::text(format!("status {}", msg.status.as_str())))
            })
        },
    )
}

fn worker_await_tool(ctx: Arc<WorkerToolContext>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "worker_await".into(),
            constrained_sampling: None,
            description: "Block until a response arrives for msg_id or timeout.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "msg_id": { "type": "string" },
                    "timeout_ms": { "type": "integer" }
                },
                "required": ["msg_id"]
            }),
        },
        "Await worker reply",
        move |_, args| {
            let ctx = ctx.clone();
            Box::pin(async move {
                let msg_id = args.get("msg_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let timeout_ms = args
                    .get("timeout_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(ctx.ask_timeout_ms);
                await_response(&ctx, &msg_id, timeout_ms).await
            })
        },
    )
}

fn worker_ask_tool(ctx: Arc<WorkerToolContext>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "worker_ask".into(),
            constrained_sampling: None,
            description: "Send a message to a peer and wait for their reply (blocks until timeout).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string" },
                    "message": { "type": "string" },
                    "timeout_ms": { "type": "integer" }
                },
                "required": ["target", "message"]
            }),
        },
        "Ask worker",
        move |_, args| {
            let ctx = ctx.clone();
            Box::pin(async move {
                let target = args.get("target").and_then(|v| v.as_str()).unwrap_or("").trim();
                let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("").trim();
                if target.is_empty() || message.is_empty() {
                    bail!("target and message are required");
                }
                let timeout_ms = args
                    .get("timeout_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(ctx.ask_timeout_ms);
                let peer = resolve_peer(&ctx, target).await?;
                let msg = ctx
                    .mailbox
                    .send_prompt(
                        &ctx.project_key,
                        &ctx.worker_id,
                        &ctx.session_id,
                        &peer.session_id,
                        Some(&peer.worker_id),
                        message,
                        0,
                        None,
                        None,
                    )
                    .await?;
                await_response(&ctx, &msg.id, timeout_ms).await
            })
        },
    )
}

async fn resolve_peer(
    ctx: &WorkerToolContext,
    target: &str,
) -> Result<super::types::LiveWorker> {
    let peers = ctx
        .registry
        .list_live_peers(&ctx.project_key, &ctx.worker_id, ctx.stale_secs)
        .await?;
    peers
        .into_iter()
        .find(|p| !p.is_self && (p.name == target || p.session_id == target || p.worker_id == target))
        .ok_or_else(|| anyhow::anyhow!("no live worker matching `{target}`"))
}

async fn await_response(ctx: &WorkerToolContext, msg_id: &str, timeout_ms: u64) -> Result<AgentToolResult> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if let Some(resp) = ctx.mailbox.get_response_for(msg_id).await? {
            let text = extract_text(&resp.payload).unwrap_or_else(|| resp.payload.clone());
            if let Some(err) = &resp.error {
                return Ok(AgentToolResult::text(format!("error: {err}\n{text}")));
            }
            return Ok(AgentToolResult::text(text));
        }
        if let Some(msg) = ctx.mailbox.get(msg_id).await? {
            if matches!(msg.status, MessageStatus::Timeout | MessageStatus::Error) {
                return Ok(AgentToolResult::text(format!(
                    "status {} {}",
                    msg.status.as_str(),
                    msg.error.unwrap_or_default()
                )));
            }
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = ctx.mailbox.mark_timeout(msg_id).await;
            bail!("timeout waiting for worker reply ({timeout_ms}ms)");
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

fn extract_text(payload: &str) -> Option<String> {
    let v: Value = serde_json::from_str(payload).ok()?;
    v.get("text")?.as_str().map(str::to_string)
}

// silence unused Pin if not needed
type _Fut = Pin<Box<dyn Future<Output = Result<AgentToolResult>> + Send>>;

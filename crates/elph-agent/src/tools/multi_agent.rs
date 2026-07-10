//! Multi-agent tools — spawn and coordinate subagents.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::subagent::AgentControl;
use crate::tools::simple_tool;
use crate::types::{AgentTool, AgentToolResult};

pub fn create_multi_agent_tools(control: Arc<AgentControl>) -> Vec<AgentTool> {
    vec![
        spawn_agent_tool(control.clone()),
        send_message_tool(control.clone()),
        followup_task_tool(control.clone()),
        wait_agent_tool(control.clone()),
        list_agents_tool(control),
    ]
}

fn spawn_agent_tool(control: Arc<AgentControl>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "spawn_agent".into(),
            description: "Spawn a subagent to handle a focused task in an isolated context.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_name": { "type": "string", "description": "Short label for the subagent task" },
                    "message": { "type": "string", "description": "Optional initial instruction" }
                },
                "required": ["task_name"]
            }),
        },
        "Spawn subagent",
        move |_, args| spawn_agent_exec(control.clone(), args),
    )
}

fn send_message_tool(control: Arc<AgentControl>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "send_message".into(),
            description: "Queue a message on a subagent without starting a turn.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "message": { "type": "string" }
                },
                "required": ["agent_id", "message"]
            }),
        },
        "Send to subagent",
        move |_, args| send_message_exec(control.clone(), args),
    )
}

fn followup_task_tool(control: Arc<AgentControl>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "followup_task".into(),
            description: "Send a message to a subagent and run a turn.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" },
                    "message": { "type": "string" }
                },
                "required": ["agent_id", "message"]
            }),
        },
        "Follow up subagent",
        move |_, args| followup_task_exec(control.clone(), args),
    )
}

fn wait_agent_tool(control: Arc<AgentControl>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "wait_agent".into(),
            description: "Wait until a subagent finishes its current turn.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string" }
                },
                "required": ["agent_id"]
            }),
        },
        "Wait for subagent",
        move |_, args| wait_agent_exec(control.clone(), args),
    )
}

fn list_agents_tool(control: Arc<AgentControl>) -> AgentTool {
    simple_tool(
        elph_ai::Tool {
            name: "list_agents".into(),
            description: "List active subagents in this session.".into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        "List subagents",
        move |_, _| list_agents_exec(control.clone()),
    )
}

fn spawn_agent_exec(
    control: Arc<AgentControl>,
    args: Value,
) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let task_name = args
            .get("task_name")
            .and_then(|v| v.as_str())
            .unwrap_or("task")
            .to_string();
        let message = args.get("message").and_then(|v| v.as_str()).map(str::to_string);
        match control.spawn_agent(task_name, message).await {
            Ok(id) => Ok(AgentToolResult::text(format!("Spawned subagent {id}"))),
            Err(error) => Ok(AgentToolResult::error(error)),
        }
    })
}

fn send_message_exec(
    control: Arc<AgentControl>,
    args: Value,
) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let agent_id = required_str(&args, "agent_id")?;
        let message = required_str(&args, "message")?;
        control
            .send_message(&agent_id, message)
            .await
            .map(|()| AgentToolResult::text("Message queued"))
            .map_err(|e| anyhow::anyhow!(e))
    })
}

fn followup_task_exec(
    control: Arc<AgentControl>,
    args: Value,
) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let agent_id = required_str(&args, "agent_id")?;
        let message = required_str(&args, "message")?;
        control
            .followup_task(&agent_id, message)
            .await
            .map(|()| AgentToolResult::text(format!("Turn started on {agent_id}")))
            .map_err(|e| anyhow::anyhow!(e))
    })
}

fn wait_agent_exec(
    control: Arc<AgentControl>,
    args: Value,
) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let agent_id = required_str(&args, "agent_id")?;
        control
            .wait_agent(&agent_id)
            .await
            .map(|()| AgentToolResult::text(format!("{agent_id} is idle")))
            .map_err(|e| anyhow::anyhow!(e))
    })
}

fn list_agents_exec(
    control: Arc<AgentControl>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<AgentToolResult>> + Send>> {
    Box::pin(async move {
        let agents = control.list_agents(None).await;
        let body = serde_json::to_string_pretty(&agents).unwrap_or_else(|_| "[]".into());
        Ok(AgentToolResult::text(body))
    })
}

fn required_str(args: &Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("Missing required argument: {key}"))
}

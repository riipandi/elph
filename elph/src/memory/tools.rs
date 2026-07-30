//! Agent-callable memory tools wrapping the floppy store.
//!
//! These tools mirror the memelord MCP tools so the agent can persist and retrieve
//! project-specific memories across sessions:
//!
//! - `memory_start_task` — retrieve relevant memories via vector search
//! - `memory_end_task` — rate retrieved memories, record task outcome
//! - `memory_report` — store a correction, user input, or insight
//! - `memory_contradict` — flag a retrieved memory as wrong and delete it
//! - `memory_status` — show memory system stats

use std::sync::Arc;

use anyhow::{Context, Result};
use elph_agent::{AgentTool, AgentToolResult};
use elph_ai::Tool;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

use super::store::open_store;
use crate::platform::Paths;

use floppy::{
    MemoryReportInput, ReportCorrectionInput, ReportUserInput, SelfReportEntry, StartTaskResult, TaskEndInput,
    UserInputSource,
};

/// Shared state for memory tools: a lazily-initialized [`floppy::MemoryStore`].
struct MemoryToolState {
    store: OnceCell<floppy::MemoryStore>,
    paths: Paths,
}

impl MemoryToolState {
    fn new(paths: Paths) -> Self {
        Self {
            store: OnceCell::new(),
            paths,
        }
    }

    async fn get_or_init(&self) -> Result<&floppy::MemoryStore> {
        self.store
            .get_or_try_init(|| async {
                let store = open_store(&self.paths, true).context("initialize memory store for tools")?;
                store.init().await.context("initialize memory store tables")?;
                Ok(store)
            })
            .await
    }
}

/// Create all memory tools wired to the given project paths.
pub fn create_memory_tools(paths: Paths) -> Vec<AgentTool> {
    let state = Arc::new(MemoryToolState::new(paths));

    vec![
        create_start_task_tool(Arc::clone(&state)),
        create_end_task_tool(Arc::clone(&state)),
        create_report_tool(Arc::clone(&state)),
        create_contradict_tool(Arc::clone(&state)),
        create_memory_status_tool(Arc::clone(&state)),
    ]
}

// ---------------------------------------------------------------------------
// memory_start_task
// ---------------------------------------------------------------------------

fn create_start_task_tool(state: Arc<MemoryToolState>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_start_task".into(),
            constrained_sampling: None,
            description: "Retrieve relevant memories for a task description via vector search. \
                          Call this at the start of every significant task to surface \
                          lessons from previous sessions."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "A concise description of the current task"
                    }
                },
                "required": ["description"]
            }),
        },
        "memory_start_task",
        move |_, args| {
            let state = Arc::clone(&state);
            Box::pin(async move { execute_start_task(state, args).await })
        },
    )
}

async fn execute_start_task(state: Arc<MemoryToolState>, args: Value) -> Result<AgentToolResult> {
    let description = args
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`description` is required"))?;

    let store = state.get_or_init().await?;
    let StartTaskResult { task_id, memories } = store.start_task(description).await?;

    let memory_list: Vec<Value> = memories
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "content": m.content,
                "category": format!("{:?}", m.category).to_lowercase(),
                "score": m.score,
                "weight": m.weight,
            })
        })
        .collect();

    let summary = if memories.is_empty() {
        "No relevant memories found for this task.".to_string()
    } else {
        format!(
            "Found {} relevant {} for this task:",
            memories.len(),
            if memories.len() == 1 { "memory" } else { "memories" },
        )
    };

    let text = if memories.is_empty() {
        summary
    } else {
        let lines: Vec<String> = memories
            .iter()
            .map(|m| {
                let preview = if m.content.len() > 200 {
                    format!("{}...", &m.content[..200])
                } else {
                    m.content.clone()
                };
                format!(
                    "- [{}] score={:.2}, weight={:.2}\n  {}",
                    format!("{:?}", m.category).to_lowercase(),
                    m.score,
                    m.weight,
                    preview,
                )
            })
            .collect();
        format!("{summary}\n\n{}", lines.join("\n"))
    };

    Ok(AgentToolResult {
        content: vec![elph_agent::ToolResultContent::Text(elph_ai::TextContent::new(text))],
        details: json!({
            "taskId": task_id,
            "memoryCount": memories.len(),
            "memories": memory_list,
        }),
        added_tool_names: None,
        terminate: None,
        usage: None,
    })
}

// ---------------------------------------------------------------------------
// memory_end_task
// ---------------------------------------------------------------------------

fn create_end_task_tool(state: Arc<MemoryToolState>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_end_task".into(),
            constrained_sampling: None,
            description: "Rate the retrieved memories and record the task outcome. \
                          Call this when a task is complete, failed, or abandoned. \
                          This updates memory weights so helpful memories survive \
                          and irrelevant ones decay."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "taskId": {
                        "type": "string",
                        "description": "The task ID returned by memory_start_task"
                    },
                    "tokensUsed": {
                        "type": "integer",
                        "description": "Approximate number of tokens used during this task"
                    },
                    "toolCalls": {
                        "type": "integer",
                        "description": "Number of tool calls made during this task"
                    },
                    "errors": {
                        "type": "integer",
                        "description": "Number of errors encountered"
                    },
                    "userCorrections": {
                        "type": "integer",
                        "description": "Number of times the user corrected the agent"
                    },
                    "completed": {
                        "type": "boolean",
                        "description": "Whether the task was completed successfully"
                    },
                    "ratings": {
                        "type": "array",
                        "description": "Ratings for each retrieved memory (0=irrelevant, 1=somewhat, 2=useful, 3=critical)",
                        "items": {
                            "type": "object",
                            "properties": {
                                "memoryId": {
                                    "type": "string",
                                    "description": "The memory ID from memory_start_task results"
                                },
                                "score": {
                                    "type": "integer",
                                    "description": "Rating 0-3",
                                    "minimum": 0,
                                    "maximum": 3
                                }
                            },
                            "required": ["memoryId", "score"]
                        }
                    }
                },
                "required": ["taskId", "tokensUsed", "toolCalls", "errors", "userCorrections", "completed"]
            }),
        },
        "memory_end_task",
        move |_, args| {
            let state = Arc::clone(&state);
            Box::pin(async move { execute_end_task(state, args).await })
        },
    )
}

async fn execute_end_task(state: Arc<MemoryToolState>, args: Value) -> Result<AgentToolResult> {
    let task_id = args
        .get("taskId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("`taskId` is required"))?
        .to_string();

    let tokens_used = args
        .get("tokensUsed")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(0);
    let tool_calls = args
        .get("toolCalls")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(0);
    let errors = args
        .get("errors")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(0);
    let user_corrections = args
        .get("userCorrections")
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(0);
    let completed = args.get("completed").and_then(Value::as_bool).unwrap_or(false);

    let self_report: Option<Vec<SelfReportEntry>> = args.get("ratings").and_then(Value::as_array).map(|arr| {
        arr.iter()
            .filter_map(|entry| {
                let memory_id = entry.get("memoryId")?.as_str()?.to_string();
                let score = entry.get("score")?.as_u64()? as u8;
                Some(SelfReportEntry { memory_id, score })
            })
            .collect()
    });

    let input = TaskEndInput {
        tokens_used,
        tool_calls,
        errors,
        user_corrections,
        completed,
        self_report,
    };

    let store = state.get_or_init().await?;
    store.end_task(&task_id, input).await?;

    Ok(AgentToolResult::text(format!(
        "Task {task_id} recorded: completed={completed}, tokens={tokens_used}, errors={errors}, corrections={user_corrections}"
    )))
}

// ---------------------------------------------------------------------------
// memory_report
// ---------------------------------------------------------------------------

fn create_report_tool(state: Arc<MemoryToolState>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_report".into(),
            constrained_sampling: None,
            description: "Store a correction, user input, or insight into persistent memory. \
                          This helps future sessions learn from this session's experience."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "description": "Type of memory: 'correction' (agent self-corrects), 'user' (user provides input), 'insight' (discovery about the codebase)",
                        "enum": ["correction", "user", "insight"]
                    },
                    "lesson": {
                        "type": "string",
                        "description": "The core lesson or information to remember"
                    },
                    "whatFailed": {
                        "type": "string",
                        "description": "What approach failed (for corrections)"
                    },
                    "whatWorked": {
                        "type": "string",
                        "description": "What approach worked instead (for corrections)"
                    },
                    "tokensWasted": {
                        "type": "integer",
                        "description": "Approximate tokens wasted on the failed approach (for corrections)"
                    },
                    "source": {
                        "type": "string",
                        "description": "Source of user input: 'user_denial', 'user_correction', 'user_input'",
                        "enum": ["user_denial", "user_correction", "user_input"]
                    }
                },
                "required": ["type", "lesson"]
            }),
        },
        "memory_report",
        move |_, args| {
            let state = Arc::clone(&state);
            Box::pin(async move { execute_report(state, args).await })
        },
    )
}

async fn execute_report(state: Arc<MemoryToolState>, args: Value) -> Result<AgentToolResult> {
    let report_type = args
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("`type` is required (correction, user, or insight)"))?;
    let lesson = args
        .get("lesson")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`lesson` is required"))?
        .to_string();

    let store = state.get_or_init().await?;

    match report_type {
        "correction" => {
            let what_failed = args
                .get("whatFailed")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let what_worked = args
                .get("whatWorked")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let tokens_wasted = args.get("tokensWasted").and_then(Value::as_u64).map(|n| n as u32);

            let id = store
                .report_correction(ReportCorrectionInput {
                    lesson: lesson.clone(),
                    what_failed,
                    what_worked,
                    tokens_wasted,
                    tools_wasted: None,
                })
                .await?;

            Ok(AgentToolResult::text(format!("Correction stored (id={id}): \"{lesson}\"")))
        }
        "user" => {
            let source = match args.get("source").and_then(Value::as_str).unwrap_or("user_input") {
                "user_denial" => UserInputSource::UserDenial,
                "user_correction" => UserInputSource::UserCorrection,
                _ => UserInputSource::UserInput,
            };

            let id = store.report_user_input(ReportUserInput { lesson, source }).await?;

            Ok(AgentToolResult::text(format!("User input stored (id={id})")))
        }
        "insight" => {
            let input = MemoryReportInput::insight(lesson.clone());
            let id = store.report(input).await?;

            Ok(AgentToolResult::text(format!("Insight stored (id={id}): \"{lesson}\"")))
        }
        other => Err(anyhow::anyhow!(
            "Unknown memory type: {other}. Use 'correction', 'user', or 'insight'."
        )),
    }
}

// ---------------------------------------------------------------------------
// memory_contradict
// ---------------------------------------------------------------------------

fn create_contradict_tool(state: Arc<MemoryToolState>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_contradict".into(),
            constrained_sampling: None,
            description: "Flag a retrieved memory as wrong and delete it. \
                          Optionally provide a correction that replaces it. \
                          Use when you find that a memory contains incorrect information."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "memoryId": {
                        "type": "string",
                        "description": "The ID of the memory to contradict"
                    },
                    "correction": {
                        "type": "string",
                        "description": "Optional correction to store in place of the deleted memory"
                    }
                },
                "required": ["memoryId"]
            }),
        },
        "memory_contradict",
        move |_, args| {
            let state = Arc::clone(&state);
            Box::pin(async move { execute_contradict(state, args).await })
        },
    )
}

async fn execute_contradict(state: Arc<MemoryToolState>, args: Value) -> Result<AgentToolResult> {
    let memory_id = args
        .get("memoryId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("`memoryId` is required"))?
        .to_string();
    let correction = args.get("correction").and_then(Value::as_str).map(str::to_string);

    let store = state.get_or_init().await?;
    let (deleted, correction_id) = store.contradict_memory(&memory_id, correction.as_deref()).await?;

    if deleted {
        let mut msg = format!("Memory {memory_id} deleted.");
        if let Some(cid) = correction_id {
            msg.push_str(&format!(" Correction stored (id={cid})."));
        }
        Ok(AgentToolResult::text(msg))
    } else {
        Ok(AgentToolResult::text(format!(
            "Memory {memory_id} not found — nothing to delete."
        )))
    }
}

// ---------------------------------------------------------------------------
// memory_status
// ---------------------------------------------------------------------------

fn create_memory_status_tool(state: Arc<MemoryToolState>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_status".into(),
            constrained_sampling: None,
            description: "Show memory system statistics: total memories, task counts, \
                          average task score, top memories by weight."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        "memory_status",
        move |_, _args| {
            let state = Arc::clone(&state);
            Box::pin(async move { execute_memory_status(state).await })
        },
    )
}

async fn execute_memory_status(state: Arc<MemoryToolState>) -> Result<AgentToolResult> {
    let store = state.get_or_init().await?;
    let stats = store.get_stats().await?;

    let text = format!(
        "Memory store status:\n\
         - Total memories: {}\n\
         - Total tasks: {}\n\
         - Avg task score: {:.3}\n\
         - Top memories:\n{}",
        stats.total_memories,
        stats.task_count,
        stats.avg_task_score,
        stats
            .top_memories
            .iter()
            .map(|m| {
                let preview = if m.content.len() > 80 {
                    format!("{}...", &m.content[..80])
                } else {
                    m.content.clone()
                };
                format!("  [w={:.2}, used={}x] {}", m.weight, m.retrieval_count, preview)
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );

    Ok(AgentToolResult::text(text))
}

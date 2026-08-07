//! Agent-callable memory tools wrapping the shared [`MemoryRuntime`].

use std::sync::Arc;

use anyhow::{Context, Result};
use elph_agent::{AgentTool, AgentToolResult};
use elph_ai::Tool;
use serde_json::{Value, json};

use floppy::{
    MemoryCategory, MemoryReportInput, ReportCorrectionInput, ReportUserInput, SelfReportEntry, StartTaskResult,
    TaskEndInput, UserInputSource,
};

use super::runtime::MemoryRuntime;

/// Create all memory tools wired to the shared runtime.
pub fn create_memory_tools(runtime: Arc<MemoryRuntime>) -> Vec<AgentTool> {
    vec![
        create_start_task_tool(Arc::clone(&runtime)),
        create_end_task_tool(Arc::clone(&runtime)),
        create_report_tool(Arc::clone(&runtime)),
        create_contradict_tool(Arc::clone(&runtime)),
        create_memory_status_tool(Arc::clone(&runtime)),
        create_search_tool(Arc::clone(&runtime)),
        create_recent_tool(runtime),
    ]
}

fn create_start_task_tool(runtime: Arc<MemoryRuntime>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_start_task".into(),
            constrained_sampling: None,
            description: "Retrieve memories for a *new* subtask description via vector search. \
                          Automatic per-turn recall already ran for the user message — call this \
                          only when pivoting to a substantially different subtask. Prefer \
                          `memory_search` / `memory_recent` for historical questions."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "A concise description of the subtask pivot"
                    }
                },
                "required": ["description"]
            }),
        },
        "memory_start_task",
        move |_, args| {
            let runtime = Arc::clone(&runtime);
            Box::pin(async move { execute_start_task(runtime, args).await })
        },
    )
}

async fn execute_start_task(runtime: Arc<MemoryRuntime>, args: Value) -> Result<AgentToolResult> {
    let description = args
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`description` is required"))?;

    let StartTaskResult { task_id, memories } = runtime.start_task(description).await?;

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
                let preview = if m.content.chars().count() > 200 {
                    let t: String = m.content.chars().take(200).collect();
                    format!("{t}...")
                } else {
                    m.content.clone()
                };
                format!(
                    "- [{}] id={} score={:.2}, weight={:.2}\n  {}",
                    format!("{:?}", m.category).to_lowercase(),
                    m.id,
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

fn create_end_task_tool(runtime: Arc<MemoryRuntime>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_end_task".into(),
            constrained_sampling: None,
            description: "Rate retrieved memories and record task outcome. Usually automatic at \
                          turn end — use only for advanced manual close of a task started with \
                          memory_start_task."
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
            let runtime = Arc::clone(&runtime);
            Box::pin(async move { execute_end_task(runtime, args).await })
        },
    )
}

async fn execute_end_task(runtime: Arc<MemoryRuntime>, args: Value) -> Result<AgentToolResult> {
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

    runtime.end_task(&task_id, input).await?;

    Ok(AgentToolResult::text(format!(
        "Task {task_id} recorded: completed={completed}, tokens={tokens_used}, errors={errors}, corrections={user_corrections}"
    )))
}

fn create_report_tool(runtime: Arc<MemoryRuntime>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_report".into(),
            constrained_sampling: None,
            description: "Store a durable correction, user preference, or insight into persistent \
                          memory. Use for lessons auto-capture would miss (architectural decisions, \
                          style preferences). Successful file edits are auto-journaled as work \
                          memories — do not re-report routine edits."
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
            let runtime = Arc::clone(&runtime);
            Box::pin(async move { execute_report(runtime, args).await })
        },
    )
}

async fn execute_report(runtime: Arc<MemoryRuntime>, args: Value) -> Result<AgentToolResult> {
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

            let id = runtime
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

            let id = runtime.report_user_input(ReportUserInput { lesson, source }).await?;

            Ok(AgentToolResult::text(format!("User input stored (id={id})")))
        }
        "insight" => {
            let input = MemoryReportInput::insight(lesson.clone());
            let id = runtime.report(input).await?;

            Ok(AgentToolResult::text(format!("Insight stored (id={id}): \"{lesson}\"")))
        }
        other => Err(anyhow::anyhow!(
            "Unknown memory type: {other}. Use 'correction', 'user', or 'insight'."
        )),
    }
}

fn create_contradict_tool(runtime: Arc<MemoryRuntime>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_contradict".into(),
            constrained_sampling: None,
            description: "Flag a retrieved memory as wrong and delete it. Optionally provide a \
                          correction that replaces it. Prefer this over silently ignoring bad recalls."
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
            let runtime = Arc::clone(&runtime);
            Box::pin(async move { execute_contradict(runtime, args).await })
        },
    )
}

async fn execute_contradict(runtime: Arc<MemoryRuntime>, args: Value) -> Result<AgentToolResult> {
    let memory_id = args
        .get("memoryId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("`memoryId` is required"))?
        .to_string();
    let correction = args.get("correction").and_then(Value::as_str).map(str::to_string);

    let (deleted, correction_id) = runtime.contradict_memory(&memory_id, correction.as_deref()).await?;

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

fn create_memory_status_tool(runtime: Arc<MemoryRuntime>) -> AgentTool {
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
            let runtime = Arc::clone(&runtime);
            Box::pin(async move { execute_memory_status(runtime).await })
        },
    )
}

async fn execute_memory_status(runtime: Arc<MemoryRuntime>) -> Result<AgentToolResult> {
    let stats = runtime.get_stats().await.context("get memory stats")?;

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
                let preview = if m.content.chars().count() > 80 {
                    let t: String = m.content.chars().take(80).collect();
                    format!("{t}...")
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

fn create_search_tool(runtime: Arc<MemoryRuntime>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_search".into(),
            constrained_sampling: None,
            description: "Semantic search across persistent memories without creating a task. \
                          Prefer this over re-scanning the filesystem for historical decisions, \
                          past work, or known layout lessons."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    }
                },
                "required": ["query"]
            }),
        },
        "memory_search",
        move |_, args| {
            let runtime = Arc::clone(&runtime);
            Box::pin(async move { execute_search(runtime, args).await })
        },
    )
}

async fn execute_search(runtime: Arc<MemoryRuntime>, args: Value) -> Result<AgentToolResult> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`query` is required"))?;

    let memories = runtime.search_memories(query).await?;
    if memories.is_empty() {
        return Ok(AgentToolResult::text(format!("No memories matched query: {query}")));
    }

    let lines: Vec<String> = memories
        .iter()
        .map(|m| {
            let preview = if m.content.chars().count() > 240 {
                let t: String = m.content.chars().take(240).collect();
                format!("{t}...")
            } else {
                m.content.clone()
            };
            format!(
                "- [{}] id={} score={:.2} w={:.2}\n  {}",
                format!("{:?}", m.category).to_lowercase(),
                m.id,
                m.score,
                m.weight,
                preview
            )
        })
        .collect();

    Ok(AgentToolResult::text(format!(
        "Search results for \"{query}\" ({}):\n\n{}",
        memories.len(),
        lines.join("\n")
    )))
}

fn create_recent_tool(runtime: Arc<MemoryRuntime>) -> AgentTool {
    elph_agent::simple_tool(
        Tool {
            name: "memory_recent".into(),
            constrained_sampling: None,
            description: "List the most recent memories (optionally by category). Use for \
                          \"what did we just change\" without semantic search. Category \
                          `work` holds auto-captured edit footprints."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Max entries (default 10)",
                        "minimum": 1,
                        "maximum": 50
                    },
                    "category": {
                        "type": "string",
                        "description": "Optional filter: correction, user, insight, discovery, work, consolidated",
                        "enum": ["correction", "user", "insight", "discovery", "work", "consolidated"]
                    }
                }
            }),
        },
        "memory_recent",
        move |_, args| {
            let runtime = Arc::clone(&runtime);
            Box::pin(async move { execute_recent(runtime, args).await })
        },
    )
}

async fn execute_recent(runtime: Arc<MemoryRuntime>, args: Value) -> Result<AgentToolResult> {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n.clamp(1, 50) as u32)
        .unwrap_or(10);
    let category = args.get("category").and_then(Value::as_str).and_then(|s| match s {
        "correction" => Some(MemoryCategory::Correction),
        "user" => Some(MemoryCategory::User),
        "insight" => Some(MemoryCategory::Insight),
        "discovery" => Some(MemoryCategory::Discovery),
        "work" => Some(MemoryCategory::Work),
        "consolidated" => Some(MemoryCategory::Consolidated),
        _ => None,
    });

    let records = runtime.list_recent_memories(limit, category).await?;
    if records.is_empty() {
        return Ok(AgentToolResult::text("No recent memories found."));
    }

    let lines: Vec<String> = records
        .iter()
        .map(|m| {
            let preview = if m.content.chars().count() > 200 {
                let t: String = m.content.chars().take(200).collect();
                format!("{t}...")
            } else {
                m.content.clone()
            };
            format!(
                "- [{}] id={} w={:.2}\n  {}",
                format!("{:?}", m.category).to_lowercase(),
                m.id,
                m.weight,
                preview
            )
        })
        .collect();

    Ok(AgentToolResult::text(format!(
        "Recent memories ({}):\n\n{}",
        records.len(),
        lines.join("\n")
    )))
}

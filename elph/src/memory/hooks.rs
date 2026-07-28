//! Memory hooks: session lifecycle, automatic recall, auto-correction, and auto task lifecycle.
//!
//! ## Existing hooks
//! - [`build_memories_context`] — inject top-weighted memories at session start.
//! - [`session_end_maintenance`] — embed pending memories + weight decay at shutdown.
//!
//! ## Automatic hooks (P1 + P2)
//! - [`register_automatic_memory_hooks`] — one-shot registration of all automatic hooks:
//!   1. **Automatic Memory Recall (before_agent_start):** per-turn semantic search,
//!      inject relevant memories into system prompt.
//!   2. **Auto-Correction (before_agent_start + on_tool_result):** detect user
//!      corrections and tool errors, persist as correction/user memories.
//!   3. **Auto Task Lifecycle (before_agent_start + TurnEnd):** auto start_task /
//!      end_task at natural turn boundaries so weight training actually runs.
//!   4. **Adaptive Recall Threshold (P2):** dynamic threshold based on total memory
//!      count and weights.

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::time::timeout;

use elph_agent::{
    AgentEvent, AgentHarness, AgentHarnessEvent, BeforeAgentStartEvent, BeforeAgentStartResult, SessionDirStorage,
    ToolResultEvent,
};
use elph_ai::Usage;
use floppy::{Memory, MemoryStore, ReportCorrectionInput, ReportUserInput, UserInputSource};

use super::store::open_store;
use crate::platform::Paths;

/// How long to wait for the database lock before giving up on startup context.
const MEMORY_STARTUP_LOCK_TIMEOUT: Duration = Duration::from_secs(8);

// ---------------------------------------------------------------------------
// Automatic memory hooks (P1 + P2)
// ---------------------------------------------------------------------------

/// Timeout for memory store operations in automatic hooks (graceful skip on lock).
const RECALL_DB_TIMEOUT: Duration = Duration::from_secs(2);

/// Minimum user query length to trigger a memory search (skip greetings, noise).
const MIN_QUERY_LENGTH: usize = 15;

/// Lazily-initialized memory store with embedder for automatic hooks.
///
/// The `tokio::sync::Mutex` guard can be held across `.await` points, unlike
/// `parking_lot::Mutex`. The inner `MemoryStore` uses `std::sync::Mutex` for
/// its own short-lived internal state (initialized, baseline, current_task_id)
/// so concurrent access is safe.
static RECALL_STORE: OnceLock<tokio::sync::Mutex<Option<MemoryStore>>> = OnceLock::new();

// Thread-local active task ID for the auto task lifecycle.
thread_local! {
    static ACTIVE_TASK_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Ensure the recall store is initialized (embedder loaded once per process).
async fn ensure_recall_store(paths: &Paths) -> Result<&tokio::sync::Mutex<Option<MemoryStore>>> {
    let cell = RECALL_STORE.get_or_init(|| tokio::sync::Mutex::new(None));
    {
        let guard = cell.lock().await;
        if guard.is_some() {
            return Ok(cell);
        }
    }
    // Open store with embedder — first call may download model weights.
    let store = open_store(paths, true)?;
    store.init().await?;
    let mut guard = cell.lock().await;
    *guard = Some(store);
    Ok(cell)
}

/// Adaptive recall threshold based on total memory count (P2).
///
/// - < 10 memories:  0.40 (permissive, cold start)
/// - 10–50 memories: 0.55
/// - 50–200 memories: 0.65 (default optimal)
/// - > 200 memories:  0.75 (strict, reduce noise)
///
/// Weight-aware boost: if any memory has weight > 3.0, threshold is lowered by 0.10
/// (proven-useful memories surface more easily).
fn adaptive_recall_threshold(total_memories: u64, memory_weights: &[f64]) -> f64 {
    let base: f64 = match total_memories {
        0..=9 => 0.40,
        10..=49 => 0.55,
        50..=199 => 0.65,
        _ => 0.75,
    };
    let max_weight = memory_weights.iter().copied().fold(0.0_f64, f64::max);
    let weight_discount: f64 = if max_weight > 3.0 { 0.10 } else { 0.0 };
    (base - weight_discount).clamp(0.30, 0.85)
}

/// Format a list of relevant memories as an XML context block for system prompt injection.
fn format_memory_context(memories: &[Memory]) -> String {
    if memories.is_empty() {
        return String::new();
    }
    let mut lines = Vec::with_capacity(memories.len() + 5);
    lines.push("<memory_context>".to_string());
    lines.push("The following are relevant lessons retrieved from previous sessions:".to_string());
    for (i, mem) in memories.iter().enumerate() {
        let preview = if mem.content.len() > 200 {
            format!("{}...", &mem.content[..200])
        } else {
            mem.content.clone()
        };
        let category = format!("{:?}", mem.category).to_lowercase();
        lines.push(format!(
            "{}. [{} | score={:.2}, w={:.2}, used={}x] {}",
            i + 1,
            category,
            mem.score,
            mem.weight,
            mem.retrieval_count,
            preview,
        ));
    }
    lines.push("Use `memory_start_task` to retrieve more and `memory_report` to store new lessons.".to_string());
    lines.push("</memory_context>".to_string());
    lines.join("\n")
}

/// Keywords that suggest a user message is a correction (not a normal continuation).
fn is_user_correction(text: &str) -> Option<&'static str> {
    let lower = text.trim().to_lowercase();
    let patterns = &[
        ("jangan", "id:jangan"),
        ("seharusnya", "id:seharusnya"),
        ("sebaiknya", "id:sebaiknya"),
        ("harusnya", "id:harusnya"),
        ("bukan begitu", "id:bukan_begitu"),
        ("salah", "id:salah"),
        ("wrong approach", "en:wrong_approach"),
        ("that's not", "en:thats_not"),
        ("that is not", "en:that_is_not"),
        ("instead, use", "en:instead_use"),
        ("actually, use", "en:actually_use"),
        ("don't", "en:dont"),
        ("do not", "en:do_not"),
        ("no,", "en:no_comma"),
    ];
    for (pat, label) in patterns {
        if lower.contains(pat) {
            return Some(label);
        }
    }
    None
}

/// Extract token usage from an [`AgentMessage`] that wraps an assistant LLM message.
fn extract_usage_from_agent_message(msg: &elph_agent::AgentMessage) -> Option<Usage> {
    let llm = msg.as_llm()?;
    match llm {
        elph_ai::Message::Assistant(assistant) => {
            if assistant.usage.total_tokens > 0 {
                Some(assistant.usage.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Format a tool error as a correction memory lesson string.
fn format_tool_error_lesson(tool_name: &str, args: &Value) -> String {
    let args_preview = serde_json::to_string(args).unwrap_or_default();
    let args_short = if args_preview.len() > 300 {
        format!("{}...", &args_preview[..300])
    } else {
        args_preview
    };
    format!("Tool `{tool_name}` failed with args: {args_short}")
}

/// Whether an [`AgentMessage`] has an error (stop_reason Error or error_message set).
fn assistant_message_has_error(msg: &elph_agent::AgentMessage) -> bool {
    let llm = match msg.as_llm() {
        Some(m) => m,
        None => return false,
    };
    match llm {
        elph_ai::Message::Assistant(assistant) => {
            assistant.error_message.is_some() || matches!(assistant.stop_reason, elph_ai::StopReason::Error)
        }
        _ => false,
    }
}

/// Register all automatic memory hooks on the harness.
///
/// 1. **Automatic memory recall (before_agent_start)** — per-turn semantic search,
///    inject most relevant memories into the system prompt.
/// 2. **User correction detection (before_agent_start)** — detect correction
///    keywords in user input, persist as correction memory.
/// 3. **Tool error auto-correction (on_tool_result)** — when a tool execution
///    fails, persist the failure as a correction memory.
/// 4. **Auto task lifecycle (before_agent_start + TurnEnd)** — auto `start_task`
///    at turn start and `end_task` at turn end so weight training runs.
pub async fn register_automatic_memory_hooks(harness: &AgentHarness<SessionDirStorage>, paths: &Paths) -> Result<()> {
    let paths = paths.clone();

    // -------------------------------------------------------------------
    // Hook A: before_agent_start — recall + user correction detection
    // -------------------------------------------------------------------
    harness
        .on_before_agent_start({
            let paths = paths.clone();
            move |event: &BeforeAgentStartEvent| {
                let paths = paths.clone();
                let prompt = event.prompt.clone();
                let system_prompt = event.system_prompt.clone();
                Box::pin(async move {
                    // --- Step 1: detect user corrections (side-effect) ---
                    if is_user_correction(&prompt).is_some()
                        && let Ok(cell) = ensure_recall_store(&paths).await
                        && let Some(store) = cell.lock().await.as_ref()
                    {
                        let lesson = format!("User correction: {}", prompt.trim());
                        let _ = store
                            .report_user_input(ReportUserInput {
                                lesson,
                                source: UserInputSource::UserCorrection,
                            })
                            .await;
                    }

                    // --- Step 2: skip short queries (greetings, noise) ---
                    if prompt.trim().chars().count() < MIN_QUERY_LENGTH {
                        return None;
                    }

                    // --- Step 3: auto start_task + semantic search ---
                    let cell = match ensure_recall_store(&paths).await {
                        Ok(c) => c,
                        Err(_) => return None,
                    };

                    let (memories, _task_id): (Vec<Memory>, Option<String>) = {
                        let mut guard = cell.lock().await;
                        let store = match guard.as_mut() {
                            Some(s) => s,
                            None => return None,
                        };

                        // Timebox the start_task + search so a stuck DB doesn't block the turn.
                        let search = timeout(RECALL_DB_TIMEOUT, store.start_task(&prompt)).await;

                        match search {
                            Ok(Ok(result)) => {
                                let tid = Some(result.task_id.clone());
                                ACTIVE_TASK_ID.with(|cell| *cell.borrow_mut() = Some(result.task_id));
                                (result.memories, tid)
                            }
                            _ => {
                                // Fallback to read-only search if start_task fails.
                                match timeout(RECALL_DB_TIMEOUT, store.search_memories(&prompt)).await {
                                    Ok(Ok(m)) => (m, None),
                                    _ => return None,
                                }
                            }
                        }
                    };

                    // --- Step 4: filter with adaptive threshold ---
                    let total_count = {
                        let guard = cell.lock().await;
                        match guard.as_ref() {
                            Some(s) => timeout(RECALL_DB_TIMEOUT, s.get_stats())
                                .await
                                .ok()
                                .and_then(|r| r.ok())
                                .map(|s| s.total_memories as u64)
                                .unwrap_or(0),
                            None => 0,
                        }
                    };

                    let weights: Vec<f64> = memories.iter().map(|m| m.weight).collect();
                    let threshold = adaptive_recall_threshold(total_count, &weights);

                    let relevant: Vec<&Memory> = memories.iter().filter(|m| m.score >= threshold).take(5).collect();

                    if relevant.is_empty() {
                        return None;
                    }

                    // --- Step 5: inject memory context into system prompt ---
                    let context = format_memory_context(&relevant.iter().map(|m| (*m).clone()).collect::<Vec<_>>());
                    let new_prompt = if system_prompt.is_empty() {
                        context
                    } else {
                        format!("{system_prompt}\n\n{context}")
                    };

                    Some(BeforeAgentStartResult {
                        system_prompt: Some(new_prompt),
                        messages: None,
                    })
                })
            }
        })
        .await;

    // -------------------------------------------------------------------
    // Hook B: on_tool_result — tool error auto-correction
    // -------------------------------------------------------------------
    harness
        .on_tool_result({
            let paths = paths.clone();
            move |event: &ToolResultEvent| {
                let paths = paths.clone();
                let tool_name = event.tool_name.clone();
                let args = event.input.clone();
                let is_error = event.is_error;
                Box::pin(async move {
                    if !is_error {
                        return None; // don't modify result
                    }
                    // Persist correction memory for the failed tool call.
                    let lesson = format_tool_error_lesson(&tool_name, &args);
                    let what_failed = format!("Tool execution error: {tool_name}");

                    if let Ok(cell) = ensure_recall_store(&paths).await
                        && let Some(store) = cell.lock().await.as_ref()
                    {
                        let _ = store
                            .report_correction(ReportCorrectionInput {
                                lesson,
                                what_failed,
                                what_worked: "unknown".into(),
                                tokens_wasted: None,
                                tools_wasted: None,
                            })
                            .await;
                    }
                    None // pass result through unchanged
                })
            }
        })
        .await;

    // -------------------------------------------------------------------
    // Hook C: subscribe — auto end_task on TurnEnd
    // -------------------------------------------------------------------
    harness
        .subscribe({
            let paths = paths.clone();
            move |event: AgentHarnessEvent, _signal| {
                let paths = paths.clone();
                Box::pin(async move {
                    // Only handle Agent → TurnEnd events.
                    let agent_event = match event {
                        AgentHarnessEvent::Agent(e) => e,
                        _ => return,
                    };
                    let (message, tool_results) = match agent_event {
                        AgentEvent::TurnEnd { message, tool_results } => (message, tool_results),
                        _ => return,
                    };

                    // Take the active task ID (set by start_task in the before_agent_start hook).
                    let task_id = ACTIVE_TASK_ID.with(|cell| cell.borrow_mut().take());
                    let Some(task_id) = task_id else {
                        return;
                    };

                    // Extract token usage from the assistant message.
                    let tokens_used = extract_usage_from_agent_message(&message)
                        .map(|u| u.output as u32)
                        .unwrap_or(0);

                    // Count tool results — each message in `tool_results` represents one
                    // tool execution. `Message::ToolResult` is the typical variant.
                    let tool_calls = tool_results.len() as u32;

                    // Count errors: we approximate by checking if the assistant has an error.
                    let has_error = assistant_message_has_error(&message);
                    let errors = if has_error { tool_calls } else { 0u32 };

                    let input = floppy::TaskEndInput {
                        tokens_used,
                        tool_calls,
                        errors,
                        user_corrections: 0,
                        completed: !has_error,
                        self_report: None,
                    };

                    if let Ok(cell) = ensure_recall_store(&paths).await
                        && let Some(store) = cell.lock().await.as_ref()
                    {
                        let _ = store.end_task(&task_id, input).await;
                    }
                })
            }
        })
        .await;

    Ok(())
}

/// Whether an error message mentions a database lock.
fn is_lock_error(err: &anyhow::Error) -> bool {
    let msg = format!("{err:#}");
    msg.contains("locked") || msg.contains("Locking") || msg.contains("database is locked") || msg.contains("BUSY")
}

/// Format top N memories by weight as a system-prompt section.
///
/// Call during session start and append the result to the system prompt so the
/// agent is aware of persistent lessons from previous sessions.
///
/// ## Lock handling
///
/// If another process holds the Turso database lock, this retries internally
/// for up to [`MEMORY_STARTUP_LOCK_TIMEOUT`]. If the lock still cannot be
/// acquired, the function returns an empty context with a warning log — the
/// agent will still work, it just won't have pre-injected memories until the
/// next session.
pub async fn build_memories_context(paths: &Paths) -> Result<String> {
    let store = match open_store(paths, false) {
        Ok(s) => s,
        Err(err) => {
            log::warn!("memory startup: open_store failed: {err:#}");
            return Ok(String::new());
        }
    };

    // Timebox the init + query so a stuck lock does not block startup indefinitely.
    let result = timeout(MEMORY_STARTUP_LOCK_TIMEOUT, async {
        store.init().await?;
        store.get_top_by_weight(5).await
    })
    .await;

    let top = match result {
        Ok(Ok(memories)) => memories,
        Ok(Err(err)) => {
            if is_lock_error(&err) {
                log::warn!(
                    "memory startup: database locked after retries — skipping memory context \
                     (memories will load lazily via memory_start_task)"
                );
            } else {
                log::warn!("memory startup: query failed: {err:#}");
            }
            return Ok(String::new());
        }
        Err(_elapsed) => {
            log::warn!(
                "memory startup: database lock timeout after {}.{}s — \
                 another process may hold the lock; skipping memory context",
                MEMORY_STARTUP_LOCK_TIMEOUT.as_secs(),
                MEMORY_STARTUP_LOCK_TIMEOUT.subsec_millis(),
            );
            return Ok(String::new());
        }
    };

    if top.is_empty() {
        return Ok(String::new());
    }

    let mut lines = Vec::with_capacity(top.len() + 3);
    lines.push("<memory_context>".to_string());
    lines.push(
        "The following are persistent lessons and knowledge from previous sessions, \
         retrieved by weight (most useful first):"
            .to_string(),
    );

    for (i, mem) in top.iter().enumerate() {
        let preview = if mem.content.len() > 160 {
            format!("{}...", &mem.content[..160])
        } else {
            mem.content.clone()
        };
        let category = format!("{:?}", mem.category).to_lowercase();
        lines.push(format!(
            "{}. [{} | w={:.2} | used={}x] {}",
            i + 1,
            category,
            mem.weight,
            mem.retrieval_count,
            preview,
        ));
    }

    lines.push(
        "Use `memory_start_task` to retrieve task-specific memories and `memory_report` \
         to store new lessons for future sessions."
            .to_string(),
    );
    lines.push("</memory_context>".to_string());

    Ok(lines.join("\n"))
}

/// Run end-of-session maintenance: embed pending memories and decay weights.
///
/// Call during session shutdown. Logs results but does not fail on errors
/// (best-effort cleanup). If the database is locked by another process,
/// maintenance is skipped — the next session start will pick up pending
/// embeddings via the lazy `memory_start_task` path.
pub async fn session_end_maintenance(paths: &Paths) {
    let store = match open_store(paths, true) {
        Ok(s) => s,
        Err(err) => {
            log::warn!("memory session-end: failed to open store: {err:#}");
            return;
        }
    };

    // Timebox init so a stuck lock does not delay shutdown.
    let init_result = timeout(Duration::from_secs(4), store.init()).await;
    match init_result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            if is_lock_error(&err) {
                log::warn!("memory session-end: database locked — skipping maintenance");
            } else {
                log::warn!("memory session-end: init failed: {err:#}");
            }
            return;
        }
        Err(_) => {
            log::warn!("memory session-end: init timed out (lock contention) — skipping maintenance");
            return;
        }
    }

    // Embed any memories that were inserted without an embedding (from hooks, raw inserts).
    match store.embed_pending().await {
        Ok(n) if n > 0 => log::info!("memory session-end: embedded {n} pending memories"),
        Ok(_) => {}
        Err(err) => log::warn!("memory session-end: embed_pending failed: {err:#}"),
    }

    // Apply weight decay so unused memories gradually fade.
    match store.decay().await {
        Ok(result) => {
            if result.decayed > 0 || result.deleted > 0 {
                log::info!(
                    "memory session-end: decay applied to {}, deleted {}",
                    result.decayed,
                    result.deleted,
                );
            }
        }
        Err(err) => log::warn!("memory session-end: decay failed: {err:#}"),
    }

    if let Err(err) = store.close().await {
        log::warn!("memory session-end: close failed: {err:#}");
    }
}

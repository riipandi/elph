//! Session lifecycle hooks for the floppy memory store.
//!
//! Mirrors the memelord lifecycle:
//! - Session start: inject top memories into agent context
//! - Session end: embed pending memories, run weight decay
//!
//! Database lock handling: both hooks handle lock contention gracefully.
//! Startup degrades to an empty context (memories are loaded lazily via tools
//! once the lock clears). Shutdown logs the error and moves on.

use anyhow::Result;
use tokio::time::{Duration, timeout};

use super::store::open_store;
use crate::platform::Paths;

/// How long to wait for the database lock before giving up on startup context.
const MEMORY_STARTUP_LOCK_TIMEOUT: Duration = Duration::from_secs(8);

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

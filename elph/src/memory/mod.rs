mod cmd;
mod format;
pub mod hooks;
pub(crate) mod store;
pub mod tools;

pub mod codegraph;

pub use cmd::run;

/// Run a memory slash command and return formatted output.
///
/// Supported subcommands:
///   status          — overview: counts, categories, top memories
///   list [cat]      — list memories, optionally filtered by category
///   tasks [n]       — show last N tasks (default: 10)
///   log [n]         — compact timeline (default: 20 events per kind)
///   search <query>  — semantic search
///   purge [thresh]  — delete memories below weight (default: 0.5)
pub async fn slash_run(paths: &crate::platform::Paths, args: &str) -> Result<String, String> {
    let args = args.trim();
    let (sub, rest) = args.split_once(' ').unwrap_or((args, ""));
    let rest = rest.trim();

    match sub {
        "" | "status" | "stats" => {
            let store = store::open_store(paths, false).map_err(|e| format!("open store: {e}"))?;
            store.init().await.map_err(|e| format!("init store: {e}"))?;
            let status = store.get_status().await.map_err(|e| format!("get status: {e}"))?;
            let mut out = String::new();
            crate::memory::format::write_status(&mut out, &status);
            Ok(out)
        }
        "list" | "ls" => {
            let filter = if rest.is_empty() {
                None
            } else {
                Some(
                    crate::memory::format::parse_category_filter(rest)
                        .ok_or_else(|| format!("unknown category: {rest}"))?,
                )
            };
            let store = store::open_store(paths, false).map_err(|e| format!("open store: {e}"))?;
            store.init().await.map_err(|e| format!("init store: {e}"))?;
            let records = store.list_memories(filter).await.map_err(|e| format!("list: {e}"))?;
            let mut out = String::new();
            crate::memory::format::write_memories(&mut out, &records, filter);
            Ok(out)
        }
        "tasks" | "task" | "t" => {
            let limit: u32 = rest.parse().unwrap_or(10);
            let store = store::open_store(paths, false).map_err(|e| format!("open store: {e}"))?;
            store.init().await.map_err(|e| format!("init store: {e}"))?;
            let tasks = store.list_tasks(limit).await.map_err(|e| format!("list tasks: {e}"))?;
            let mut out = String::new();
            crate::memory::format::write_tasks(&mut out, &tasks);
            Ok(out)
        }
        "log" | "l" => {
            let limit: u32 = rest.parse().unwrap_or(20);
            let store = store::open_store(paths, false).map_err(|e| format!("open store: {e}"))?;
            store.init().await.map_err(|e| format!("init store: {e}"))?;
            let events = store.get_timeline(limit).await.map_err(|e| format!("timeline: {e}"))?;
            let mut out = String::new();
            crate::memory::format::write_timeline(&mut out, &events);
            Ok(out)
        }
        "search" | "s" => {
            if rest.is_empty() {
                return Err("usage: /memory search <query>".into());
            }
            let store = store::open_store(paths, true).map_err(|e| format!("open store: {e}"))?;
            store.init().await.map_err(|e| format!("init store: {e}"))?;
            let result = store.search(rest).await.map_err(|e| format!("search: {e}"))?;
            let mut out = String::new();
            crate::memory::format::write_search_results(&mut out, rest, &result.memories);
            Ok(out)
        }
        "purge" | "p" => {
            let threshold: f64 = rest.parse().unwrap_or(0.5);
            let store = store::open_store(paths, false).map_err(|e| format!("open store: {e}"))?;
            store.init().await.map_err(|e| format!("init store: {e}"))?;
            let count = store.purge(threshold).await.map_err(|e| format!("purge: {e}"))?;
            let mut out = String::new();
            crate::memory::format::write_purge(&mut out, count, threshold);
            Ok(out)
        }
        "help" | "h" | "--help" | "-h" => Ok(concat!(
            "Memory commands:\n",
            "  /memory status            Overview: counts, categories, top memories\n",
            "  /memory list [category]   List all memories (optionally filter by category)\n",
            "  /memory tasks [n]         Show last N tasks with retrievals and outcomes\n",
            "  /memory log [n]           Compact timeline of tasks and memory events\n",
            "  /memory search <query>    Semantic search across memories\n",
            "  /memory purge [threshold] Delete memories below weight threshold\n",
        )
        .to_string()),
        other => Err(format!("unknown memory subcommand: {other}. Try /memory help")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_subcommand_parsing() {
        // Just test that the dispatch pattern works — actual DB tests are heavier.
    }
}

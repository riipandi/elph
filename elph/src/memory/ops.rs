//! Shared execution for `elph memory` CLI and `/memory` slash commands.

use anyhow::{Context, Result, bail};

use floppy::MemoryCategory;

use super::format::{
    category_help_list, parse_category_filter, write_consolidate, write_flush, write_help, write_memories, write_note,
    write_purge, write_search_results, write_status, write_tasks, write_timeline, MemoryStyle,
};
use super::store::open_store;
use crate::platform::{Paths, Settings};

/// Parsed memory inspection/maintenance command (CLI + slash share this).
#[derive(Debug, Clone)]
pub enum MemoryOp {
    Status,
    List {
        category: Option<MemoryCategory>,
        limit: u32,
    },
    Recent {
        category: Option<MemoryCategory>,
        limit: u32,
    },
    Tasks {
        limit: u32,
    },
    Log {
        limit: u32,
    },
    Search {
        query: String,
    },
    Purge {
        threshold: f64,
    },
    /// Wipe all memories + tasks. Caller must confirm before running.
    Flush,
    Consolidate,
    Help,
}

impl MemoryOp {
    /// Parse slash args after `/memory` (e.g. `recent 5 work`, `search foo bar`).
    pub fn parse_slash(args: &str) -> Result<Self, String> {
        let args = args.trim();
        if args.is_empty() {
            return Ok(Self::Status);
        }
        let mut parts = args.split_whitespace();
        let sub = parts.next().unwrap_or("").to_ascii_lowercase();
        let rest: Vec<&str> = parts.collect();

        match sub.as_str() {
            "status" | "stats" | "info" => Ok(Self::Status),
            "help" | "h" | "--help" | "-h" => Ok(Self::Help),
            "list" | "ls" => {
                let (category, limit) = parse_category_and_limit(&rest, 50)?;
                Ok(Self::List { category, limit })
            }
            "recent" | "r" => {
                let (category, limit) = parse_category_and_limit(&rest, 10)?;
                Ok(Self::Recent { category, limit })
            }
            "tasks" | "task" | "t" => {
                let limit = parse_optional_u32(rest.first().copied()).unwrap_or(10);
                Ok(Self::Tasks {
                    limit: limit.clamp(1, 100),
                })
            }
            "log" | "l" | "timeline" => {
                let limit = parse_optional_u32(rest.first().copied()).unwrap_or(20);
                Ok(Self::Log {
                    limit: limit.clamp(1, 200),
                })
            }
            "search" | "s" | "find" => {
                if rest.is_empty() {
                    return Err("usage: /memory search <query>".into());
                }
                Ok(Self::Search { query: rest.join(" ") })
            }
            "purge" | "p" => {
                let threshold = rest.first().and_then(|s| s.parse().ok()).unwrap_or(0.5);
                if !(0.0..=5.0).contains(&threshold) {
                    return Err("purge threshold must be between 0 and 5".into());
                }
                Ok(Self::Purge { threshold })
            }
            "flush" | "clear" | "wipe" => Ok(Self::Flush),
            "consolidate" | "merge" | "dedupe" => Ok(Self::Consolidate),
            other => Err(format!("unknown memory subcommand: {other}. Try /memory help")),
        }
    }
}

fn parse_optional_u32(s: Option<&str>) -> Option<u32> {
    s.and_then(|v| v.parse().ok())
}

/// Tokens can be `[category] [n]`, `[n] [category]`, just `n`, or just `category`.
fn parse_category_and_limit(parts: &[&str], default_limit: u32) -> Result<(Option<MemoryCategory>, u32), String> {
    let mut category = None;
    let mut limit = default_limit;
    for p in parts {
        if let Ok(n) = p.parse::<u32>() {
            limit = n.clamp(1, 200);
            continue;
        }
        if let Some(c) = parse_category_filter(p) {
            category = Some(c);
            continue;
        }
        return Err(format!("unknown token {p:?}. Categories: {}", category_help_list()));
    }
    Ok((category, limit))
}

/// Run a memory op and return formatted text (no printing).
///
/// Uses plain (no ANSI) styling — suitable for slash dialogs.
pub async fn execute(paths: &Paths, op: MemoryOp) -> Result<String> {
    execute_with_style(paths, op, MemoryStyle::plain()).await
}

/// Same as [`execute`], with explicit color style (CLI uses [`MemoryStyle::auto_stdout`]).
pub async fn execute_with_style(paths: &Paths, op: MemoryOp, sty: MemoryStyle) -> Result<String> {
    let mut out = String::new();
    match op {
        MemoryOp::Help => {
            write_help(&mut out, sty);
            return Ok(out);
        }
        MemoryOp::Status => {
            let settings = Settings::load(paths).ok();
            let mem = settings.as_ref().map(|s| &s.memory);
            let store = open_store(paths, false)?;
            store.init().await?;
            // Repair corrupt zero-vectors so pending count is accurate.
            let cleared = store.clear_zero_embeddings().await.unwrap_or(0);
            let status = store.get_status().await?;
            write_status(&mut out, &status, mem, sty);
            if cleared > 0 {
                use std::fmt::Write;
                let _ = writeln!(out);
                write_note(
                    &mut out,
                    &format!(
                        "Note: reset {cleared} invalid zero-vector embedding(s) (were written by an older bug)."
                    ),
                    sty,
                );
                write_note(
                    &mut out,
                    "      Run `elph memory search <q>` or start a session to re-embed them.",
                    sty,
                );
            }
        }
        MemoryOp::List { category, limit } => {
            let store = open_store(paths, false)?;
            store.init().await?;
            let mut records = store.list_memories(category).await?;
            records.truncate(limit as usize);
            write_memories(&mut out, &records, category, sty);
        }
        MemoryOp::Recent { category, limit } => {
            let store = open_store(paths, false)?;
            store.init().await?;
            let records = store.list_recent_memories(limit, category).await?;
            write_memories(&mut out, &records, category, sty);
        }
        MemoryOp::Tasks { limit } => {
            let store = open_store(paths, false)?;
            store.init().await?;
            let tasks = store.list_tasks(limit).await?;
            write_tasks(&mut out, &tasks, sty);
        }
        MemoryOp::Log { limit } => {
            let store = open_store(paths, false)?;
            store.init().await?;
            let events = store.get_timeline(limit).await?;
            write_timeline(&mut out, &events, sty);
        }
        MemoryOp::Search { query } => {
            if query.trim().is_empty() {
                bail!("search query is empty");
            }
            let store = open_store(paths, true).context("open store with embedder for search")?;
            store.init().await?;
            // Repair any zero embeddings left by older noop embed_pending runs.
            let cleared = store.clear_zero_embeddings().await.unwrap_or(0);
            // Backfill pending so search can match newly stored work/lessons.
            let embedded = store.embed_pending().await.unwrap_or(0);
            let memories = store.search_memories(&query).await?;
            write_search_results(&mut out, &query, &memories, sty);
            if cleared > 0 || embedded > 0 {
                use std::fmt::Write;
                let _ = writeln!(out);
                if cleared > 0 {
                    write_note(
                        &mut out,
                        &format!(
                            "Note: reset {cleared} invalid zero-vector embedding(s) before search."
                        ),
                        sty,
                    );
                }
                if embedded > 0 {
                    write_note(
                        &mut out,
                        &format!("Note: embedded {embedded} pending memor(ies) for search."),
                        sty,
                    );
                }
            }
        }
        MemoryOp::Purge { threshold } => {
            let store = open_store(paths, false)?;
            store.init().await?;
            let count = store.purge(threshold).await?;
            write_purge(&mut out, count, threshold, sty);
        }
        MemoryOp::Flush => {
            let store = open_store(paths, false)?;
            store.init().await?;
            let result = store.flush().await?;
            write_flush(&mut out, result.memories, result.tasks, sty);
        }
        MemoryOp::Consolidate => {
            // Use a real embedder so pending rows are not filled with zeros.
            let store = open_store(paths, true).context("open store with embedder for consolidate")?;
            store.init().await?;
            let cleared = store.clear_zero_embeddings().await.unwrap_or(0);
            let embedded = store.embed_pending().await.unwrap_or(0);
            let result = store.consolidate_similar(0.08, 10).await?;
            write_consolidate(&mut out, result.merged, result.deleted, sty);
            if cleared > 0 || embedded > 0 {
                use std::fmt::Write;
                let _ = writeln!(out);
                if cleared > 0 {
                    write_note(
                        &mut out,
                        &format!("Note: reset {cleared} invalid zero-vector embedding(s)."),
                        sty,
                    );
                }
                if embedded > 0 {
                    write_note(
                        &mut out,
                        &format!("Note: embedded {embedded} pending memor(ies)."),
                        sty,
                    );
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slash_status_default() {
        let op = MemoryOp::parse_slash("").unwrap();
        assert!(matches!(op, MemoryOp::Status));
    }

    #[test]
    fn parse_slash_recent_category_and_limit() {
        let op = MemoryOp::parse_slash("recent work 5").unwrap();
        match op {
            MemoryOp::Recent { category, limit } => {
                assert_eq!(category, Some(MemoryCategory::Work));
                assert_eq!(limit, 5);
            }
            _ => panic!("expected recent"),
        }
        let op2 = MemoryOp::parse_slash("recent 3 discovery").unwrap();
        match op2 {
            MemoryOp::Recent { category, limit } => {
                assert_eq!(category, Some(MemoryCategory::Discovery));
                assert_eq!(limit, 3);
            }
            _ => panic!("expected recent"),
        }
    }

    #[test]
    fn parse_slash_flush() {
        assert!(matches!(MemoryOp::parse_slash("flush").unwrap(), MemoryOp::Flush));
        assert!(matches!(MemoryOp::parse_slash("clear").unwrap(), MemoryOp::Flush));
        assert!(matches!(MemoryOp::parse_slash("wipe").unwrap(), MemoryOp::Flush));
    }

    #[test]
    fn parse_slash_search_multiword() {
        let op = MemoryOp::parse_slash("search auth middleware").unwrap();
        match op {
            MemoryOp::Search { query } => assert_eq!(query, "auth middleware"),
            _ => panic!("expected search"),
        }
    }
}

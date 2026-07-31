use anyhow::{Context, Result};
use elph_agent::try_block_on;

use super::format::{MemoryStyle, parse_category_filter, write_flush_cancelled};
use super::ops::{MemoryOp, execute_with_style};
use crate::cli::MemoryCommands;
use crate::cli::interactive;
use crate::platform::Paths;

pub fn run(paths: Paths, cmd: &MemoryCommands) -> Result<()> {
    if matches!(cmd, MemoryCommands::Flush) {
        return run_flush(paths);
    }
    let op = cli_to_op(cmd)?;
    let out = try_block_on(execute_with_style(&paths, op, MemoryStyle::auto_stdout()))??;
    print!("{out}");
    Ok(())
}

fn run_flush(paths: Paths) -> Result<()> {
    let sty = MemoryStyle::auto_stdout();
    let (memories, tasks) = try_block_on(super::flush_preview(&paths))?;
    if !interactive::confirm_memory_flush(memories, tasks) {
        let mut out = String::new();
        write_flush_cancelled(&mut out, sty);
        print!("{out}");
        return Ok(());
    }
    let out = try_block_on(execute_with_style(&paths, MemoryOp::Flush, sty))??;
    print!("{out}");
    Ok(())
}

fn cli_to_op(cmd: &MemoryCommands) -> Result<MemoryOp> {
    Ok(match cmd {
        MemoryCommands::Status => MemoryOp::Status,
        MemoryCommands::List { category, limit } => {
            let filter = match category.as_deref() {
                Some(raw) => Some(parse_category_filter(raw).with_context(|| format!("unknown category {raw:?}"))?),
                None => None,
            };
            MemoryOp::List {
                category: filter,
                limit: (*limit).clamp(1, 200),
            }
        }
        MemoryCommands::Recent { category, limit } => {
            let filter = match category.as_deref() {
                Some(raw) => Some(parse_category_filter(raw).with_context(|| format!("unknown category {raw:?}"))?),
                None => None,
            };
            MemoryOp::Recent {
                category: filter,
                limit: (*limit).clamp(1, 200),
            }
        }
        MemoryCommands::Tasks { limit } => MemoryOp::Tasks {
            limit: (*limit).clamp(1, 100),
        },
        MemoryCommands::Log { limit } => MemoryOp::Log {
            limit: (*limit).clamp(1, 200),
        },
        MemoryCommands::Search { query } => {
            if query.is_empty() {
                anyhow::bail!("usage: elph memory search <query>");
            }
            MemoryOp::Search { query: query.join(" ") }
        }
        MemoryCommands::Purge { threshold } => {
            if !(0.0..=5.0).contains(threshold) {
                anyhow::bail!("purge threshold must be between 0 and 5");
            }
            MemoryOp::Purge { threshold: *threshold }
        }
        MemoryCommands::Flush => MemoryOp::Flush,
        MemoryCommands::Consolidate => MemoryOp::Consolidate,
    })
}

use anyhow::{Context, Result, bail};
use elph_agent::try_block_on;

use super::format::{
    parse_category_filter, print_memories, print_purge, print_search_results, print_status, print_tasks, print_timeline,
};
use super::store::open_store;
use crate::cli::MemoryCommands;
use crate::platform::Paths;

pub fn run(paths: Paths, cmd: &MemoryCommands) -> Result<()> {
    match cmd {
        MemoryCommands::Status => {
            let store = open_store(&paths, false)?;
            try_block_on(async {
                store.init().await?;
                let status = store.get_status().await?;
                print_status(&status);
                Ok(())
            })?
        }
        MemoryCommands::List { category } => {
            let filter = match category.as_deref() {
                Some(raw) => Some(parse_category_filter(raw).with_context(|| format!("unknown category {raw:?}"))?),
                None => None,
            };
            let store = open_store(&paths, false)?;
            try_block_on(async {
                store.init().await?;
                let records = store.list_memories(filter).await?;
                print_memories(&records, filter);
                Ok(())
            })?
        }
        MemoryCommands::Tasks { limit } => {
            let store = open_store(&paths, false)?;
            try_block_on(async {
                store.init().await?;
                let tasks = store.list_tasks(*limit).await?;
                print_tasks(&tasks);
                Ok(())
            })?
        }
        MemoryCommands::Log { limit } => {
            let store = open_store(&paths, false)?;
            try_block_on(async {
                store.init().await?;
                let events = store.get_timeline(*limit).await?;
                print_timeline(&events);
                Ok(())
            })?
        }
        MemoryCommands::Search { query } => {
            if query.is_empty() {
                bail!("usage: elph memory search <query>");
            }
            let q = query.join(" ");
            let store = open_store(&paths, true)?;
            try_block_on(async {
                store.init().await?;
                let result = store.search(&q).await?;
                print_search_results(&q, &result.memories);
                Ok(())
            })?
        }
        MemoryCommands::Purge { threshold } => {
            let store = open_store(&paths, false)?;
            try_block_on(async {
                store.init().await?;
                let count = store.purge(*threshold).await?;
                print_purge(count, *threshold);
                Ok(())
            })?
        }
    }
}

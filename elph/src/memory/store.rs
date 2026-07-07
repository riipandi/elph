use std::sync::Arc;

use anyhow::{Context, Result};
use elph_core::memz::{EmbedFn, FastEmbedOptions, MemoryStore, MemzConfig, create_fastembed, create_memory_store};

use crate::runtime::Paths;

/// No-op embedder for read-only CLI commands (status, list, purge).
fn noop_embedder() -> EmbedFn {
    Arc::new(|_| Box::pin(async { Ok(vec![0.0f32; elph_core::memz::DEFAULT_EMBEDDING_DIMS as usize]) }))
}

fn local_embedder() -> Result<EmbedFn> {
    create_fastembed(FastEmbedOptions::default()).context("failed to initialize local embedder")
}

pub fn open_store(paths: &Paths, needs_embed: bool) -> Result<MemoryStore> {
    std::fs::create_dir_all(paths.project_elph_dir())
        .with_context(|| format!("create {}", paths.project_elph_dir().display()))?;

    let embed = if needs_embed {
        local_embedder()?
    } else {
        noop_embedder()
    };

    let config = MemzConfig::new(paths.memory_db_path().to_string_lossy().into_owned(), "elph-cli");

    Ok(create_memory_store(config, embed))
}

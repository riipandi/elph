//! Open a [`floppy::CodegraphStore`] for the current project.

use crate::platform::{GpuAcceleration, Paths, Settings};
use crate::utils::path::AppPaths;
use anyhow::{Context, Result};
use floppy::{
    CodegraphConfig, CodegraphStore, DEFAULT_EMBEDDING_DIMS, EMBEDDER_INIT_TIMEOUT, EmbedOptions, GpuBackend,
    GpuConfig, create_embedder_with_timeout, embedding_dims, noop_embedder, resolve_embedding_model,
};
use std::sync::Arc;
use turso::Database;

/// Open codegraph store sharing `<project>/.elph/store.db` with memory.
///
/// When `needs_embed` is true, uses the same MiniLM settings as floppy memory.
pub fn open_store(paths: &Paths, needs_embed: bool) -> Result<CodegraphStore> {
    open_store_with_db(paths, needs_embed, None)
}

/// Open codegraph store with an optional shared database handle.
///
/// When `database` is provided, the store connects from that shared handle
/// instead of opening the store file itself — the host owns the open/apply-
/// migrations lifetime.
pub fn open_store_with_db(paths: &Paths, needs_embed: bool, database: Option<Arc<Database>>) -> Result<CodegraphStore> {
    std::fs::create_dir_all(paths.project_elph_dir())
        .with_context(|| format!("create {}", paths.project_elph_dir().display()))?;

    let settings = Settings::load(paths).context("load settings")?;
    let model_id = settings.models.embed.model.to_model_id();
    let dims = resolve_embedding_model(&model_id, settings.models.embed.quantized)
        .map(|m| embedding_dims(&m))
        .unwrap_or(DEFAULT_EMBEDDING_DIMS);

    let embed = if needs_embed {
        std::fs::create_dir_all(paths.models_dir())
            .with_context(|| format!("create {}", paths.models_dir().display()))?;

        // Configure GPU based on user preference and hardware availability
        let gpu_config = match settings.models.embed.gpu_acceleration {
            GpuAcceleration::On => GpuConfig::with_preference(true),
            GpuAcceleration::Off => GpuConfig::with_preference(false),
            GpuAcceleration::Auto => GpuConfig::detect(),
        };
        let device = gpu_config.candle_device().map(|d| d.to_string());

        let options = EmbedOptions {
            model: Some(model_id),
            quantized: settings.models.embed.quantized,
            cache_dir: Some(paths.models_dir()),
            device,
        };
        // Bounded: a blocked Hugging Face download must fail with a helpful
        // message instead of hanging the CLI at "Preparing embedder…".
        create_embedder_with_timeout(options, EMBEDDER_INIT_TIMEOUT).context("create codegraph embedder")?
    } else {
        noop_embedder(dims)
    };

    let root = paths.project_dir().to_string_lossy().into_owned();
    let db_path = paths.memory_db_path().to_string_lossy().into_owned();

    // Determine GPU acceleration mode for display
    let gpu_acceleration = if needs_embed {
        let gpu_config = match settings.models.embed.gpu_acceleration {
            GpuAcceleration::On => GpuConfig::with_preference(true),
            GpuAcceleration::Off => GpuConfig::with_preference(false),
            GpuAcceleration::Auto => GpuConfig::detect(),
        };
        let gpu_backend = gpu_config.available_backend;
        let gpu_enabled = gpu_config.enabled;
        let setting_mode = settings.models.embed.gpu_acceleration;

        let gpu_status = if !gpu_enabled {
            format!("{} (disabled)", setting_mode)
        } else {
            match gpu_backend {
                GpuBackend::Metal => format!("{} (metal)", setting_mode),
                GpuBackend::Cuda => format!("{} (cuda)", setting_mode),
                GpuBackend::None => format!("{} (cpu)", setting_mode),
            }
        };
        Some(gpu_status)
    } else {
        None
    };

    // Clamp user-tunable batch settings so a hand-edited 0 can't hang the index
    // run or cause a divide-by-zero. Defaults match floppy's CodegraphConfig::new.
    let clamp_usize = |v: usize, default: usize, name: &str| -> usize {
        if v == 0 {
            log::warn!("codegraph.{name} is 0; falling back to default {default}");
            default
        } else {
            v
        }
    };
    let clamp_u32 = |v: u32, default: u32, name: &str| -> u32 {
        if v == 0 {
            log::warn!("codegraph.{name} is 0; falling back to default {default}");
            default
        } else {
            v
        }
    };
    let clamp_u64 = |v: u64, default: u64, name: &str| -> u64 {
        if v == 0 {
            log::warn!("codegraph.{name} is 0; falling back to default {default}");
            default
        } else {
            v
        }
    };
    // `maxDbConnections: 0` is the dangerous one: it flows into
    // `ConnectionPool::new(db, 0)` → `Semaphore::new(0)`, and `acquire()` then
    // blocks forever (the "stuck at Building codegraph index" report). The other
    // two zeros silently yield an empty/broken index (0-byte cap skips every
    // file; 0 chunk lines splits nothing). Clamp all of them to floppy's
    // `CodegraphConfig::new` defaults so a hand-edited 0 can't hang or no-op.
    let max_db_connections = clamp_usize(settings.codegraph.max_db_connections, 4, "maxDbConnections");
    let max_chunk_lines = clamp_u32(settings.codegraph.max_chunk_lines, 120, "maxChunkLines");
    let max_file_bytes = clamp_u64(settings.codegraph.max_file_bytes, 512 * 1024, "maxFileBytes");
    let embed_batch_size = clamp_usize(settings.codegraph.embed_batch_size, 64, "embedBatchSize");
    let db_commit_batch_files = clamp_usize(settings.codegraph.db_commit_batch_files, 200, "dbCommitBatchFiles");
    let embed_concurrency = clamp_usize(settings.codegraph.embed_concurrency, 1, "embedConcurrency");

    let cg_config = CodegraphConfig {
        db_path,
        root_dir: root,
        embed,
        apply_migrations: true,
        max_chunk_lines,
        max_file_bytes,
        max_db_connections: Some(max_db_connections),
        embed_batch_size,
        db_commit_batch_files,
        embed_concurrency,
        gpu_acceleration,
        database,
        include_patterns: settings.codegraph.include_patterns.clone(),
    };
    Ok(CodegraphStore::new(cg_config))
}

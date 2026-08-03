//! Open a [`floppy::CodegraphStore`] for the current project.

use anyhow::{Context, Result};
use floppy::{
    CodegraphConfig, CodegraphStore, DEFAULT_EMBEDDING_DIMS, EMBEDDER_INIT_TIMEOUT, EmbedOptions, GpuConfig,
    create_embedder_with_timeout, embedding_dims, noop_embedder, resolve_embedding_model,
};

use crate::platform::{Paths, Settings};
use crate::utils::path::AppPaths;

/// Open codegraph store sharing `<project>/.elph/store.db` with memory.
///
/// When `needs_embed` is true, uses the same MiniLM settings as floppy memory.
pub fn open_store(paths: &Paths, needs_embed: bool) -> Result<CodegraphStore> {
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
        let gpu_config = GpuConfig::with_preference(settings.models.embed.gpu);
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
    let cg_config = CodegraphConfig {
        db_path,
        root_dir: root,
        embed,
        apply_migrations: true,
        max_chunk_lines: settings.codegraph.max_chunk_lines,
        max_file_bytes: settings.codegraph.max_file_bytes,
        max_db_connections: Some(settings.codegraph.max_db_connections),
    };
    Ok(CodegraphStore::new(cg_config))
}

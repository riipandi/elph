use anyhow::{Context, Result};

use crate::utils::path::AppPaths;
use floppy::{DEFAULT_EMBEDDING_DIMS, EmbedOptions, FloppyBuilder, MemoryStore};
use floppy::{GpuConfig, embedding_dims, resolve_embedding_model};

use crate::platform::{Paths, Settings};

/// Open a floppy memory store for the current project.
///
/// When `needs_embed` is true, the store is configured with a local embedding model
/// (downloading model weights on first use). When false, a noop embedder is used
/// for read-only operations (status, list, tasks, timeline, purge).
pub fn open_store(paths: &Paths, needs_embed: bool) -> Result<MemoryStore> {
    open_store_with_session(paths, needs_embed, "elph-cli")
}

/// Open a store with an explicit session id (coding session attribution).
pub fn open_store_with_session(paths: &Paths, needs_embed: bool, session_id: &str) -> Result<MemoryStore> {
    std::fs::create_dir_all(paths.project_elph_dir())
        .with_context(|| format!("create {}", paths.project_elph_dir().display()))?;

    let settings = Settings::load(paths).context("load settings")?;
    let model_id = settings.models.embed.model.to_model_id();

    let dims = resolve_embedding_model(&model_id, settings.models.embed.quantized)
        .map(|m| embedding_dims(&m))
        .unwrap_or(DEFAULT_EMBEDDING_DIMS);

    let session = if session_id.trim().is_empty() {
        "elph-cli"
    } else {
        session_id
    };

    let top_k = settings.memory.top_k.max(1);

    let mut builder = FloppyBuilder::new(paths.memory_db_path().to_string_lossy().into_owned(), session)
        .dimensions(dims)
        .top_k(top_k)
        .apply_migrations(true);

    if needs_embed {
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
        builder = builder.embed(options)?;
    } else {
        builder = builder.noop_embed();
    }

    builder.build().context("open memory store")
}

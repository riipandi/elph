use anyhow::{Context, Result};

use elph_core::memz::{DEFAULT_EMBEDDING_DIMS, FastEmbedOptions, MemoryStore, MemzBuilder};
use elph_core::memz::{embedding_dims, resolve_embedding_model};
use elph_core::utils::path::AppPaths;

use crate::runtime::{Paths, Settings};

pub fn open_store(paths: &Paths, needs_embed: bool) -> Result<MemoryStore> {
    std::fs::create_dir_all(paths.project_elph_dir())
        .with_context(|| format!("create {}", paths.project_elph_dir().display()))?;

    let settings = Settings::load(paths).context("load settings")?;

    let dims = resolve_embedding_model(&settings.memory.embed_model, settings.memory.embed_quantized)
        .map(|m| embedding_dims(&m))
        .unwrap_or(DEFAULT_EMBEDDING_DIMS);

    let mut builder = MemzBuilder::new(paths.memory_db_path().to_string_lossy().into_owned(), "elph-cli")
        .dimensions(dims)
        .apply_migrations(false);

    if needs_embed {
        std::fs::create_dir_all(paths.models_dir())
            .with_context(|| format!("create {}", paths.models_dir().display()))?;

        let options = FastEmbedOptions {
            model: Some(settings.memory.embed_model.clone()),
            quantized: settings.memory.embed_quantized,
            cache_dir: Some(paths.models_dir()),
            show_download_progress: Some(true),
        };
        builder = builder.fastembed(options)?;
    } else {
        builder = builder.noop_embed();
    }

    builder.build().context("open memory store")
}

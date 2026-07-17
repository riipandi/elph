use anyhow::{Context, Result};

use crate::utils::path::AppPaths;
use floppy::{DEFAULT_EMBEDDING_DIMS, EmbedOptions, FloppyBuilder, MemoryStore};
use floppy::{embedding_dims, resolve_embedding_model};

use crate::platform::{Paths, Settings};

pub fn open_store(paths: &Paths, needs_embed: bool) -> Result<MemoryStore> {
    std::fs::create_dir_all(paths.project_elph_dir())
        .with_context(|| format!("create {}", paths.project_elph_dir().display()))?;

    let settings = Settings::load(paths).context("load settings")?;

    let dims = resolve_embedding_model(&settings.memory.embed_model, settings.memory.embed_quantized)
        .map(|m| embedding_dims(&m))
        .unwrap_or(DEFAULT_EMBEDDING_DIMS);

    let mut builder = FloppyBuilder::new(paths.memory_db_path().to_string_lossy().into_owned(), "elph-cli")
        .dimensions(dims)
        .apply_migrations(false);

    if needs_embed {
        std::fs::create_dir_all(paths.models_dir())
            .with_context(|| format!("create {}", paths.models_dir().display()))?;

        let options = EmbedOptions {
            model: Some(settings.memory.embed_model.clone()),
            quantized: settings.memory.embed_quantized,
            cache_dir: Some(paths.models_dir()),
        };
        builder = builder.embed(options)?;
    } else {
        builder = builder.noop_embed();
    }

    builder.build().context("open memory store")
}

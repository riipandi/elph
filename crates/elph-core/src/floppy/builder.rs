use anyhow::{Result, bail};

use super::embed::{EmbedOptions, create_embedder};

#[cfg(feature = "embed")]
use super::embed::DEFAULT_EMBED_MODEL;
#[cfg(feature = "embed")]
use super::embed::{embedding_dims, resolve_embedding_model};
use super::paths::FloppyPaths;
use super::store::{EmbedFn, MemoryStore, noop_embedder};
use super::types::{FloppyConfig, VectorType};
use super::util::DEFAULT_EMBEDDING_DIMS;

/// Builder for a [`MemoryStore`] with explicit configuration (no environment variables).
pub struct FloppyBuilder {
    config: FloppyConfig,
    embed_fn: Option<EmbedFn>,
    embed_opts: Option<EmbedOptions>,
}

impl FloppyBuilder {
    pub fn new(db_path: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            config: FloppyConfig::new(db_path, session_id),
            embed_fn: None,
            embed_opts: None,
        }
    }

    pub fn from_paths(paths: &FloppyPaths, session_id: impl Into<String>) -> Self {
        Self::new(paths.db_path_string(), session_id)
    }

    pub fn vector_type(mut self, vector_type: VectorType) -> Self {
        self.config = self.config.vector_type(vector_type);
        self
    }

    pub fn dimensions(mut self, dimensions: u32) -> Self {
        self.config = self.config.dimensions(dimensions);
        self
    }

    pub fn top_k(mut self, top_k: u32) -> Self {
        self.config = self.config.top_k(top_k);
        self
    }

    pub fn learning_rate(mut self, learning_rate: f64) -> Self {
        self.config = self.config.learning_rate(learning_rate);
        self
    }

    pub fn decay_rate(mut self, decay_rate: f64) -> Self {
        self.config = self.config.decay_rate(decay_rate);
        self
    }

    /// Skip floppy migrations in [`MemoryStore::init`] when the host already applied them.
    pub fn apply_migrations(mut self, apply: bool) -> Self {
        self.config = self.config.apply_migrations(apply);
        self
    }

    /// Custom embedder; mutually exclusive with [`Self::embed`].
    pub fn embed_fn(mut self, embed: EmbedFn) -> Self {
        self.embed_fn = Some(embed);
        self.embed_opts = None;
        self
    }

    /// Zero-vector embedder for read-only inspection without a local model.
    pub fn noop_embed(mut self) -> Self {
        let dims = self.config.dimensions.unwrap_or(DEFAULT_EMBEDDING_DIMS);
        self.embed_fn = Some(noop_embedder(dims));
        self.embed_opts = None;
        self
    }

    /// Local embedder via embed_anything. Sets [`FloppyConfig::dimensions`] from the resolved model.
    #[cfg(feature = "embed")]
    pub fn embed(mut self, options: EmbedOptions) -> Result<Self> {
        let model_name = options.model.as_deref().unwrap_or(DEFAULT_EMBED_MODEL);
        let dims = resolve_embedding_model(model_name, options.quantized)
            .map(|m| embedding_dims(&m))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.config = self.config.dimensions(dims);
        self.embed_opts = Some(options);
        self.embed_fn = None;
        Ok(self)
    }

    #[cfg(not(feature = "embed"))]
    pub fn embed(self, _options: EmbedOptions) -> Result<Self> {
        bail!("local embedder requires the `embed` feature on this crate");
    }

    pub fn build(self) -> Result<MemoryStore> {
        let embed = match (self.embed_fn, self.embed_opts) {
            (Some(e), None) => e,
            (None, Some(opts)) => create_embedder(opts)?,
            (None, None) => {
                bail!("embedder required: call .embed_fn(), .noop_embed(), or .embed()");
            }
            (Some(_), Some(_)) => bail!("cannot set both a custom embedder and embed options"),
        };
        Ok(MemoryStore::new(self.config, embed))
    }
}

impl FloppyPaths {
    /// Start a [`FloppyBuilder`] rooted at this data directory.
    pub fn builder(&self, session_id: impl Into<String>) -> FloppyBuilder {
        FloppyBuilder::from_paths(self, session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn mock_embed() -> EmbedFn {
        Arc::new(|_| Box::pin(async { Ok(vec![1.0, 0.0, 0.0, 0.0]) }))
    }

    #[test]
    fn builder_requires_embedder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("t.db").to_string_lossy().into_owned();
        match FloppyBuilder::new(db, "s").build() {
            Err(e) => assert!(e.to_string().contains("embedder required")),
            Ok(_) => panic!("expected embedder required error"),
        }
    }

    #[test]
    fn builder_with_custom_embed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("t.db").to_string_lossy().into_owned();
        let store = FloppyBuilder::new(db, "s")
            .dimensions(4)
            .embed_fn(mock_embed())
            .build()
            .expect("build");
        assert_eq!(store.dimensions(), 4);
    }

    #[test]
    fn from_paths_sets_db_location() {
        let paths = FloppyPaths::project_local();
        let store = paths.builder("sess").dimensions(4).noop_embed().build().expect("build");
        assert_eq!(store.dimensions(), 4);
    }
}

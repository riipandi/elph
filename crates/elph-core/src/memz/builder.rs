use anyhow::{Result, bail};

use super::embed::{DEFAULT_EMBED_MODEL, FastEmbedOptions, create_fastembed};
#[cfg(feature = "fastembed")]
use super::embed::{embedding_dims, resolve_embedding_model};
use super::paths::MemzPaths;
use super::store::{EmbedFn, MemoryStore, noop_embedder};
use super::types::{MemzConfig, VectorType};
use super::util::DEFAULT_EMBEDDING_DIMS;

/// Builder for a [`MemoryStore`] with explicit configuration (no environment variables).
pub struct MemzBuilder {
    config: MemzConfig,
    custom_embed: Option<EmbedFn>,
    fastembed_opts: Option<FastEmbedOptions>,
}

impl MemzBuilder {
    pub fn new(db_path: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            config: MemzConfig::new(db_path, session_id),
            custom_embed: None,
            fastembed_opts: None,
        }
    }

    pub fn from_paths(paths: &MemzPaths, session_id: impl Into<String>) -> Self {
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

    /// Skip memz migrations in [`MemoryStore::init`] when the host already applied them.
    pub fn apply_migrations(mut self, apply: bool) -> Self {
        self.config = self.config.apply_migrations(apply);
        self
    }

    /// Custom embedder; mutually exclusive with [`Self::fastembed`].
    pub fn embed(mut self, embed: EmbedFn) -> Self {
        self.custom_embed = Some(embed);
        self.fastembed_opts = None;
        self
    }

    /// Zero-vector embedder for read-only inspection without a local model.
    pub fn noop_embed(mut self) -> Self {
        let dims = self.config.dimensions.unwrap_or(DEFAULT_EMBEDDING_DIMS);
        self.custom_embed = Some(noop_embedder(dims));
        self.fastembed_opts = None;
        self
    }

    /// Local fastembed backend. Sets [`MemzConfig::dimensions`] from the resolved model.
    #[cfg(feature = "fastembed")]
    pub fn fastembed(mut self, options: FastEmbedOptions) -> Result<Self> {
        let model_name = options.model.as_deref().unwrap_or(DEFAULT_EMBED_MODEL);
        let dims = resolve_embedding_model(model_name, options.quantized)
            .map(|m| embedding_dims(&m))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.config = self.config.dimensions(dims);
        self.fastembed_opts = Some(options);
        self.custom_embed = None;
        Ok(self)
    }

    #[cfg(not(feature = "fastembed"))]
    pub fn fastembed(mut self, _options: FastEmbedOptions) -> Result<Self> {
        bail!("fastembed backend requires the `fastembed` feature on this crate");
    }

    pub fn build(self) -> Result<MemoryStore> {
        let embed = match (self.custom_embed, self.fastembed_opts) {
            (Some(e), None) => e,
            (None, Some(opts)) => create_fastembed(opts)?,
            (None, None) => {
                bail!("embedder required: call .embed(), .noop_embed(), or .fastembed()");
            }
            (Some(_), Some(_)) => bail!("cannot set both a custom embedder and fastembed options"),
        };
        Ok(MemoryStore::new(self.config, embed))
    }
}

impl MemzPaths {
    /// Start a [`MemzBuilder`] rooted at this data directory.
    pub fn builder(&self, session_id: impl Into<String>) -> MemzBuilder {
        MemzBuilder::from_paths(self, session_id)
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
        match MemzBuilder::new(db, "s").build() {
            Err(e) => assert!(e.to_string().contains("embedder required")),
            Ok(_) => panic!("expected embedder required error"),
        }
    }

    #[test]
    fn builder_with_custom_embed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("t.db").to_string_lossy().into_owned();
        let store = MemzBuilder::new(db, "s")
            .dimensions(4)
            .embed(mock_embed())
            .build()
            .expect("build");
        assert_eq!(store.dimensions(), 4);
    }

    #[test]
    fn from_paths_sets_db_location() {
        let paths = MemzPaths::project_local();
        let store = paths.builder("sess").dimensions(4).noop_embed().build().expect("build");
        assert_eq!(store.dimensions(), 4);
    }
}

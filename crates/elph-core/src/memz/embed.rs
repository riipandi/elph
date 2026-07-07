//! Optional local embedding backends for [`EmbedFn`].
//!
//! Enable with the `fastembed` feature (all-MiniLM-L6-v2).

use super::store::EmbedFn;

#[cfg(feature = "fastembed")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "fastembed")]
use super::store::EmbedFuture;

#[cfg(feature = "fastembed")]
use super::util::DEFAULT_EMBEDDING_DIMS;

/// Environment variable to override the embedding model name.
pub const ENV_EMBED_MODEL: &str = "MEMZ_EMBED_MODEL";

/// Options for the fastembed-backed local embedder.
#[derive(Debug, Clone)]
pub struct FastEmbedOptions {
    /// Use quantized ONNX weights (default: true).
    pub quantized: bool,
    /// Override model — defaults to `EmbeddingModel::AllMiniLML6V2`.
    pub model: Option<String>,
}

impl Default for FastEmbedOptions {
    fn default() -> Self {
        Self {
            quantized: true,
            model: None,
        }
    }
}

/// Create a shared local embedder using [fastembed](https://github.com/Anush008/fastembed-rs).
///
/// Default model: **all-MiniLM-L6-v2** (384 dimensions).
/// Inference runs on a blocking thread pool; safe to call from async contexts.
#[cfg(feature = "fastembed")]
pub fn create_fastembed(options: FastEmbedOptions) -> anyhow::Result<EmbedFn> {
    use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

    let model_name = options
        .model
        .or_else(|| std::env::var(ENV_EMBED_MODEL).ok())
        .unwrap_or_else(|| "AllMiniLML6V2".into());

    let embedding_model = match model_name.as_str() {
        "AllMiniLML6V2" | "sentence-transformers/all-MiniLM-L6-v2" => EmbeddingModel::AllMiniLML6V2,
        other => {
            anyhow::bail!("unsupported fastembed model {other:?}; use AllMiniLML6V2 or enable a custom backend");
        }
    };

    let init = TextInitOptions::new(embedding_model);
    let model = TextEmbedding::try_new(init)?;

    let shared = Arc::new(Mutex::new(model));
    Ok(Arc::new(move |text: &str| {
        let shared = Arc::clone(&shared);
        let text = text.to_string();
        Box::pin(async move {
            let vec = tokio::task::spawn_blocking(move || {
                let mut model = shared
                    .lock()
                    .map_err(|e| anyhow::anyhow!("embedder lock poisoned: {e}"))?;
                let embeddings = model.embed(vec![text], None)?;
                embeddings
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("fastembed returned no vectors"))
            })
            .await??;
            if vec.len() != DEFAULT_EMBEDDING_DIMS as usize {
                anyhow::bail!("expected {}-dim embedding, got {}", DEFAULT_EMBEDDING_DIMS, vec.len());
            }
            Ok(vec)
        }) as EmbedFuture
    }))
}

#[cfg(not(feature = "fastembed"))]
pub fn create_fastembed(_options: FastEmbedOptions) -> anyhow::Result<EmbedFn> {
    anyhow::bail!("fastembed embedder requires the `fastembed` feature on elph-core");
}

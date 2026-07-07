//! Optional local embedding backends for [`EmbedFn`].
//!
//! Enable with the `fastembed` feature (all-MiniLM-L6-v2 and other ONNX models).

use std::path::PathBuf;

use super::store::EmbedFn;

#[cfg(feature = "fastembed")]
use std::str::FromStr;
#[cfg(feature = "fastembed")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "fastembed")]
use super::store::EmbedFuture;

/// Default embedding model when none is configured.
pub const DEFAULT_EMBED_MODEL: &str = "AllMiniLML6V2";

/// Options for the fastembed-backed local embedder.
#[derive(Debug, Clone)]
pub struct FastEmbedOptions {
    /// Use the quantized ONNX variant when available (default: true).
    pub quantized: bool,
    /// Model name — fastembed [`EmbeddingModel`] debug name or common HF alias.
    pub model: Option<String>,
    /// ONNX/tokenizer cache directory (default: fastembed's `.fastembed_cache`).
    pub cache_dir: Option<PathBuf>,
    /// Show Hugging Face download progress (default: true).
    pub show_download_progress: Option<bool>,
}

impl Default for FastEmbedOptions {
    fn default() -> Self {
        Self {
            quantized: true,
            model: None,
            cache_dir: None,
            show_download_progress: None,
        }
    }
}

impl FastEmbedOptions {
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn cache_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    pub fn quantized(mut self, quantized: bool) -> Self {
        self.quantized = quantized;
        self
    }

    pub fn show_download_progress(mut self, show: bool) -> Self {
        self.show_download_progress = Some(show);
        self
    }
}

/// Resolve a user-facing model name to a fastembed [`EmbeddingModel`].
#[cfg(feature = "fastembed")]
pub fn resolve_embedding_model(name: &str, quantized: bool) -> Result<fastembed::EmbeddingModel, String> {
    use fastembed::EmbeddingModel;

    let trimmed = name.trim();
    let canonical = alias_model_name(trimmed);
    let mut model = EmbeddingModel::from_str(canonical)?;

    if quantized {
        model = prefer_quantized_variant(model);
    }

    Ok(model)
}

#[cfg(feature = "fastembed")]
fn alias_model_name(name: &str) -> &str {
    match name.to_ascii_lowercase().as_str() {
        "sentence-transformers/all-minilm-l6-v2" | "all-minilm-l6-v2" => "AllMiniLML6V2",
        "sentence-transformers/all-minilm-l12-v2" | "all-minilm-l12-v2" => "AllMiniLML12V2",
        "sentence-transformers/all-mpnet-base-v2" => "AllMpnetBaseV2",
        "baai/bge-small-en-v1.5" | "bge-small-en-v1.5" => "BGESmallENV15",
        "baai/bge-base-en-v1.5" | "bge-base-en-v1.5" => "BGEBaseENV15",
        "baai/bge-large-en-v1.5" | "bge-large-en-v1.5" => "BGELargeENV15",
        "nomic-ai/nomic-embed-text-v1" => "NomicEmbedTextV1",
        "nomic-ai/nomic-embed-text-v1.5" => "NomicEmbedTextV15",
        "xenova/all-minilm-l6-v2" => "AllMiniLML6V2Q",
        _ => name,
    }
}

#[cfg(feature = "fastembed")]
fn prefer_quantized_variant(model: fastembed::EmbeddingModel) -> fastembed::EmbeddingModel {
    use fastembed::EmbeddingModel;
    use std::str::FromStr;

    let debug = format!("{model:?}");
    if debug.ends_with('Q') {
        return model;
    }

    let q_name = format!("{debug}Q");
    EmbeddingModel::from_str(&q_name).unwrap_or(model)
}

/// Embedding output dimensions for a resolved model.
#[cfg(feature = "fastembed")]
pub fn embedding_dims(model: &fastembed::EmbeddingModel) -> u32 {
    use fastembed::TextEmbedding;

    TextEmbedding::get_model_info(model)
        .map(|info| info.dim as u32)
        .unwrap_or(super::util::DEFAULT_EMBEDDING_DIMS)
}

/// Create a shared local embedder using [fastembed](https://github.com/Anush008/fastembed-rs).
///
/// Default model: **AllMiniLML6V2** (quantized by default → `AllMiniLML6V2Q`).
/// Inference runs on a blocking thread pool; safe to call from async contexts.
#[cfg(feature = "fastembed")]
pub fn create_fastembed(options: FastEmbedOptions) -> anyhow::Result<EmbedFn> {
    use fastembed::{TextEmbedding, TextInitOptions};

    let model_name = options.model.unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());

    let embedding_model =
        resolve_embedding_model(&model_name, options.quantized).map_err(|e| anyhow::anyhow!("{e}"))?;

    let expected_dims = embedding_dims(&embedding_model) as usize;

    let mut init = TextInitOptions::new(embedding_model);
    if let Some(dir) = options.cache_dir {
        init = init.with_cache_dir(dir);
    }
    if let Some(show) = options.show_download_progress {
        init = init.with_show_download_progress(show);
    }

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
            if vec.len() != expected_dims {
                anyhow::bail!("expected {expected_dims}-dim embedding, got {}", vec.len());
            }
            Ok(vec)
        }) as EmbedFuture
    }))
}

#[cfg(not(feature = "fastembed"))]
pub fn create_fastembed(_options: FastEmbedOptions) -> anyhow::Result<EmbedFn> {
    anyhow::bail!("fastembed embedder requires the `fastembed` feature");
}

#[cfg(feature = "fastembed")]
#[cfg(test)]
mod tests {
    use super::*;
    use fastembed::EmbeddingModel;

    #[test]
    fn resolves_hf_alias() {
        let m = resolve_embedding_model("sentence-transformers/all-MiniLM-L6-v2", false).unwrap();
        assert_eq!(m, EmbeddingModel::AllMiniLML6V2);
    }

    #[test]
    fn quantized_prefers_q_variant() {
        let m = resolve_embedding_model("AllMiniLML6V2", true).unwrap();
        assert_eq!(m, EmbeddingModel::AllMiniLML6V2Q);
    }

    #[test]
    fn quantized_skips_already_quantized() {
        let m = resolve_embedding_model("AllMiniLML6V2Q", true).unwrap();
        assert_eq!(m, EmbeddingModel::AllMiniLML6V2Q);
    }

    #[test]
    fn resolves_bge_alias() {
        let m = resolve_embedding_model("BAAI/bge-small-en-v1.5", true).unwrap();
        assert_eq!(m, EmbeddingModel::BGESmallENV15Q);
    }

    #[test]
    fn embedding_dims_matches_model() {
        let m = EmbeddingModel::AllMiniLML6V2;
        assert_eq!(embedding_dims(&m), 384);
    }
}

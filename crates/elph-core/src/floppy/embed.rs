//! Optional local embedding backends for [`EmbedFn`].
//!
//! Enable with the `embed` feature (Candle / Hugging Face models).

use std::path::PathBuf;

use super::store::EmbedFn;

#[cfg(feature = "embed")]
use std::str::FromStr;
#[cfg(feature = "embed")]
use std::sync::Arc;

#[cfg(feature = "embed")]
use super::store::EmbedFuture;

/// Default embedding model when none is configured.
pub const DEFAULT_EMBED_MODEL: &str = "AllMiniLML6V2";

/// Resolved local embedding model (Hugging Face repo id + vector dimensions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEmbeddingModel {
    pub hf_model_id: String,
    pub dimensions: u32,
}

/// Options for the local embedder ([`embed_anything`](https://github.com/StarlightSearch/EmbedAnything)).
#[derive(Debug, Clone)]
pub struct EmbedOptions {
    /// Prefer a quantized variant when a catalog alias exists (default: true).
    pub quantized: bool,
    /// Model name — catalog id (`AllMiniLML6V2`) or Hugging Face repo id.
    pub model: Option<String>,
    /// Hugging Face cache directory (sets `HF_HOME` during embedder init).
    pub cache_dir: Option<PathBuf>,
}

impl Default for EmbedOptions {
    fn default() -> Self {
        Self {
            quantized: true,
            model: None,
            cache_dir: None,
        }
    }
}

impl EmbedOptions {
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
}

/// Resolve a user-facing model name to a Hugging Face embedding model.
#[cfg(feature = "embed")]
pub fn resolve_embedding_model(name: &str, quantized: bool) -> Result<ResolvedEmbeddingModel, String> {
    use embed_anything::embeddings::local::text_embedding::{ONNXModel, get_model_info, get_model_info_by_hf_id};

    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("embedding model name is empty".to_string());
    }

    if let Ok(mut model) = ONNXModel::from_str(alias_model_name(trimmed)) {
        if quantized {
            model = prefer_quantized_variant(model);
        }
        let info = get_model_info(&model).ok_or_else(|| format!("unknown catalog model: {model:?}"))?;
        return Ok(ResolvedEmbeddingModel {
            hf_model_id: info.hf_model_id.clone(),
            dimensions: info.dim as u32,
        });
    }

    if trimmed.contains('/') {
        let dims = get_model_info_by_hf_id(trimmed)
            .map(|info| info.dim as u32)
            .unwrap_or(super::util::DEFAULT_EMBEDDING_DIMS);
        return Ok(ResolvedEmbeddingModel {
            hf_model_id: trimmed.to_string(),
            dimensions: dims,
        });
    }

    Err(format!("unsupported embedding model: {trimmed}"))
}

#[cfg(feature = "embed")]
fn alias_model_name(name: &str) -> &str {
    match name.to_ascii_lowercase().as_str() {
        "sentence-transformers/all-minilm-l6-v2" | "all-minilm-l6-v2" => "AllMiniLML6V2",
        "sentence-transformers/all-minilm-l12-v2" | "all-minilm-l12-v2" => "AllMiniLML12V2",
        "baai/bge-small-en-v1.5" | "bge-small-en-v1.5" => "BGESmallENV15",
        "baai/bge-base-en-v1.5" | "bge-base-en-v1.5" => "BGEBaseENV15",
        "baai/bge-large-en-v1.5" | "bge-large-en-v1.5" => "BGELargeENV15",
        "nomic-ai/nomic-embed-text-v1" => "NomicEmbedTextV1",
        "nomic-ai/nomic-embed-text-v1.5" => "NomicEmbedTextV15",
        "xenova/all-minilm-l6-v2" => "AllMiniLML6V2Q",
        _ => name,
    }
}

#[cfg(feature = "embed")]
fn prefer_quantized_variant(
    model: embed_anything::embeddings::local::text_embedding::ONNXModel,
) -> embed_anything::embeddings::local::text_embedding::ONNXModel {
    use embed_anything::embeddings::local::text_embedding::ONNXModel;
    use std::str::FromStr;

    let debug = format!("{model:?}");
    if debug.ends_with('Q') {
        return model;
    }

    let q_name = format!("{debug}Q");
    ONNXModel::from_str(&q_name).unwrap_or(model)
}

/// Embedding output dimensions for a resolved model.
#[cfg(feature = "embed")]
pub fn embedding_dims(model: &ResolvedEmbeddingModel) -> u32 {
    model.dimensions
}

/// Create a shared local embedder using [embed_anything](https://github.com/StarlightSearch/EmbedAnything).
///
/// Default model: **AllMiniLML6V2** (maps to `sentence-transformers/all-MiniLM-L6-v2`).
/// Model weights download on first use into `HF_HOME` (override via [`EmbedOptions::cache_dir`]).
#[cfg(feature = "embed")]
pub fn create_embedder(options: EmbedOptions) -> anyhow::Result<EmbedFn> {
    use embed_anything::embeddings::embed::Embedder;
    use embed_anything::embeddings::local::text_embedding::ONNXModel;

    let model_name = options.model.unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
    let resolved = resolve_embedding_model(&model_name, options.quantized).map_err(|e| anyhow::anyhow!("{e}"))?;
    let expected_dims = resolved.dimensions as usize;
    let hf_model_id = resolved.hf_model_id.clone();

    let pooling = ONNXModel::from_str(alias_model_name(&model_name)).ok().and_then(|m| {
        let m = if options.quantized {
            prefer_quantized_variant(m)
        } else {
            m
        };
        m.get_default_pooling_method()
    });

    if let Some(dir) = &options.cache_dir {
        set_hf_home(dir);
    }

    let embedder = Embedder::from_pretrained_hf(&hf_model_id, None, None, None, pooling)?;

    let shared = Arc::new(embedder);
    Ok(Arc::new(move |text: &str| {
        let shared = Arc::clone(&shared);
        let text = text.to_string();
        Box::pin(async move {
            let vec = shared.embed(&[text.as_str()], Some(1), None).await?.into_iter().next();
            let vec = match vec {
                Some(result) => result.to_dense()?,
                None => anyhow::bail!("embed_anything returned no vectors"),
            };
            if vec.len() != expected_dims {
                anyhow::bail!("expected {expected_dims}-dim embedding, got {}", vec.len());
            }
            Ok(vec)
        }) as EmbedFuture
    }))
}

#[cfg(feature = "embed")]
fn set_hf_home(dir: &std::path::Path) {
    let value = dir.to_string_lossy().into_owned();
    unsafe {
        std::env::set_var("HF_HOME", value);
    }
}

#[cfg(not(feature = "embed"))]
pub fn create_embedder(_options: EmbedOptions) -> anyhow::Result<EmbedFn> {
    anyhow::bail!("local embedder requires the `embed` feature");
}

#[cfg(feature = "embed")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_hf_alias() {
        let m = resolve_embedding_model("sentence-transformers/all-MiniLM-L6-v2", false).unwrap();
        assert_eq!(m.hf_model_id, "sentence-transformers/all-MiniLM-L6-v2");
        assert_eq!(m.dimensions, 384);
    }

    #[test]
    fn quantized_prefers_q_variant() {
        let m = resolve_embedding_model("AllMiniLML6V2", true).unwrap();
        assert_eq!(m.hf_model_id, "Xenova/all-MiniLM-L6-v2");
        assert_eq!(m.dimensions, 384);
    }

    #[test]
    fn quantized_skips_already_quantized() {
        let m = resolve_embedding_model("AllMiniLML6V2Q", true).unwrap();
        assert_eq!(m.hf_model_id, "Xenova/all-MiniLM-L6-v2");
    }

    #[test]
    fn resolves_bge_alias() {
        let m = resolve_embedding_model("BAAI/bge-small-en-v1.5", true).unwrap();
        assert_eq!(m.hf_model_id, "Qdrant/bge-small-en-v1.5-onnx-Q");
        assert_eq!(m.dimensions, 384);
    }

    #[test]
    fn embedding_dims_matches_model() {
        let m = resolve_embedding_model("AllMiniLML6V2", false).unwrap();
        assert_eq!(embedding_dims(&m), 384);
    }

    #[test]
    fn accepts_raw_hf_model_id() {
        let m = resolve_embedding_model("sentence-transformers/all-MiniLM-L12-v2", false).unwrap();
        assert_eq!(m.hf_model_id, "sentence-transformers/all-MiniLM-L12-v2");
        assert_eq!(m.dimensions, 384);
    }
}

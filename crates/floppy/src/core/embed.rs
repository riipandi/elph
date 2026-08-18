//! Optional local embedding backends for [`EmbedFn`].
//!
//! Enable with the `embed` feature (Candle / Hugging Face models).

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

#[cfg(feature = "embed")]
use std::str::FromStr;

/// Future returned by [`EmbedFn`].
pub type EmbedFuture = Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>>> + Send>>;

/// Shared embedder callback used by the memory domain.
///
/// Batch-first: callers pass a slice of texts and receive one embedding vector
/// per input, in the same order. Batching many chunks into one call is the
/// dominant indexing-speedup (the embedder amortizes overhead across the batch).
pub type EmbedFn = Arc<dyn Fn(&[String]) -> EmbedFuture + Send + Sync>;

/// Embedder that returns zero vectors (read-only inspection without a model).
pub fn noop_embedder(dimensions: u32) -> EmbedFn {
    Arc::new(move |texts: &[String]| {
        let dims = dimensions as usize;
        let n = texts.len();
        Box::pin(async move { Ok(vec![vec![0.0f32; dims]; n]) })
    })
}

/// Default embedding model when none is configured.
pub const DEFAULT_EMBED_MODEL: &str = "AllMiniLML6V2";

/// Resolved local embedding model (Hugging Face repo id + vector dimensions).
#[cfg(feature = "embed")]
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
    /// Advisory GPU device hint for reporting (e.g. `"metal"`, `"cuda:0"`, or `None`=CPU).
    /// Actual device selection is compile-time via the `metal` / `cuda` cargo features
    /// (embed_anything 0.7.1 has no runtime device parameter), so this does not change which
    /// device the embedder runs on — it feeds `gpu_acceleration` stats reported upstream.
    pub device: Option<String>,
}

impl Default for EmbedOptions {
    fn default() -> Self {
        Self {
            quantized: true,
            model: None,
            cache_dir: None,
            device: None,
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

    pub fn device(mut self, device: impl Into<String>) -> Self {
        self.device = Some(device.into());
        self
    }
}

/// Resolve a user-facing model name to a Hugging Face embedding model.
#[cfg(feature = "embed")]
pub fn resolve_embedding_model(name: &str, quantized: bool) -> Result<ResolvedEmbeddingModel, String> {
    use embed_anything::embeddings::local::text_embedding::ONNXModel;
    use embed_anything::embeddings::local::text_embedding::{get_model_info, get_model_info_by_hf_id};

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
        "jinaai/jina-embeddings-v2-base-en" | "jina-embeddings-v2-base-en" => "JinaEmbeddingsV2BaseEn",
        "jinaai/jina-embeddings-v2-small-en" | "jina-embeddings-v2-small-en" => "JinaEmbeddingsV2SmallEn",
        "qwen/qwen3-embedding-0.6b" | "qwen3-embedding-0.6b" => "Qwen3Embedding06B",
        "google/embeddinggemma-300m" | "gemma-embedding-300m" => "GemmaEmbedding300M",
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
///
/// **Note:** This function uses `embed_anything`'s Candle-based `from_pretrained_hf` path, which
/// requires models with PyTorch/safetensors weights. ONNX-only Q-variant models (`Xenova/`,
/// `Qdrant/` repos) are not supported here — the `quantized` option is ignored for model resolution.
///
/// **GPU Support:** Enable the `embed-gpu` feature for Apple Metal (macOS ARM64) or `embed-cuda` for
/// NVIDIA CUDA (Linux/Windows). GPU is selected automatically at compile time via cargo features.
/// The `device` option is currently ignored.
#[cfg(feature = "embed")]
pub fn create_embedder(options: EmbedOptions) -> anyhow::Result<EmbedFn> {
    use embed_anything::embeddings::embed::Embedder;
    use embed_anything::embeddings::local::text_embedding::ONNXModel;

    let model_name = options.model.unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
    // Candle's from_pretrained_hf cannot load ONNX-only Q-variant models (Xenova, Qdrant repos
    // only have ONNX weights). Always resolve the non-quantized variant for Candle compatibility.
    let resolved = resolve_embedding_model(&model_name, false).map_err(|e| anyhow::anyhow!("{e}"))?;
    let expected_dims = resolved.dimensions as usize;
    let hf_model_id = resolved.hf_model_id.clone();

    // Pooling strategy is identical between Q and non-Q variants for the same model.
    let base_model = ONNXModel::from_str(alias_model_name(&model_name)).unwrap_or(
        // Fallback to default catalog model if name doesn't match an alias.
        ONNXModel::AllMiniLML6V2,
    );
    let pooling = base_model.get_default_pooling_method();

    if let Some(dir) = &options.cache_dir {
        set_hf_home(dir);
    }

    // Device is selected at compile time by the `metal` / `cuda` cargo features
    // (see embed_anything's `select_device`). embed_anything 0.7.1 has no runtime
    // device parameter on `from_pretrained_hf` (the 4th slot is the data `dtype`,
    // which the Candle/Bert path ignores), so `options.device` is advisory only —
    // it still feeds `gpu_acceleration` reporting upstream. dtype defaults to F32.
    log::info!(
        "embedder loading model={hf_model_id} dims={expected_dims} quantized_opt={}",
        options.quantized
    );
    let embedder = Embedder::from_pretrained_hf(&hf_model_id, None, None, None, pooling)
        .inspect_err(|err| log::error!("embedder load failed model={hf_model_id}: {err:#}"))?;
    log::info!("embedder ready model={hf_model_id} dims={expected_dims}");

    let shared = Arc::new(embedder);
    Ok(Arc::new(move |texts: &[String]| {
        let shared = Arc::clone(&shared);
        let owned: Vec<String> = texts.to_vec();
        Box::pin(async move {
            let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
            let results = shared.embed(&refs, Some(refs.len()), None).await?;
            let mut out = Vec::with_capacity(results.len());
            for r in results {
                let vec = r.to_dense()?;
                if vec.len() != expected_dims {
                    anyhow::bail!("expected {expected_dims}-dim embedding, got {}", vec.len());
                }
                out.push(vec);
            }
            Ok(out)
        }) as EmbedFuture
    }))
}

/// Default cap on embedder initialization (model weights download) before failing.
///
/// [`Embedder::from_pretrained_hf`] performs a synchronous Hugging Face download
/// with no internal deadline. On a blocked or blackholed network it can hang the
/// CLI at "Preparing embedder…" indefinitely; callers that must not stall should
/// use [`create_embedder_with_timeout`].
pub const EMBEDDER_INIT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Create a local embedder, bounding the potentially slow weights download to
/// `timeout`. The first download for a model is cached under `HF_HOME`, so the
/// timeout only bites on genuinely slow or unreachable networks.
///
/// On timeout the worker thread keeps running in the background (it cannot be
/// force-killed); the caller should treat the error as fatal and abort the
/// operation, letting the process exit drop the stray thread.
#[cfg(feature = "embed")]
pub fn create_embedder_with_timeout(options: EmbedOptions, timeout: Duration) -> anyhow::Result<EmbedFn> {
    run_with_init_timeout(timeout, move || create_embedder(options))
}

/// Run a blocking embedder-initialization closure with a hard `timeout`.
///
/// The closure runs on a dedicated OS thread so a hung network download cannot
/// stall the caller; `recv_timeout` bounds the wait.
#[cfg(feature = "embed")]
fn run_with_init_timeout(
    timeout: Duration,
    init: impl FnOnce() -> anyhow::Result<EmbedFn> + Send + 'static,
) -> anyhow::Result<EmbedFn> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("embedder-init".to_string())
        .spawn(move || {
            let _ = tx.send(init());
        })?;
    match rx.recv_timeout(timeout) {
        Ok(Ok(embed)) => Ok(embed),
        Ok(Err(err)) => {
            log::error!("embedder init failed: {err:#}");
            Err(err)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            log::error!("embedder init timed out after {}s", timeout.as_secs());
            Err(anyhow::anyhow!(
                "embedder initialization timed out after {}s — the model weights download is \
             slow or the network is blocked. Check your connection and retry; after the first \
             success the model is cached locally.",
                timeout.as_secs()
            ))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            log::error!("embedder init thread exited unexpectedly");
            Err(anyhow::anyhow!("embedder initialization thread exited unexpectedly"))
        }
    }
}

#[cfg(feature = "embed")]
fn set_hf_home(dir: &std::path::Path) {
    let value = dir.to_string_lossy().into_owned();
    // SAFETY: Called once during embedder initialization, before any
    // concurrent access to env var state. No other thread reads HF_HOME
    // concurrently; the embedder is not yet built.
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

    #[test]
    fn default_embed_options() {
        let opts = EmbedOptions::default();
        assert!(opts.quantized);
        assert!(opts.model.is_none());
        assert!(opts.cache_dir.is_none());
    }

    #[test]
    fn resolve_unknown_model_returns_err() {
        assert!(resolve_embedding_model("nonexistent-model-v99", false).is_err());
    }

    #[test]
    fn init_error_propagates_promptly() {
        // A model that fails fast at resolution must surface immediately, not
        // after the full timeout window.
        let result = create_embedder_with_timeout(
            EmbedOptions {
                model: Some("nonexistent-model-v99".to_string()),
                ..EmbedOptions::default()
            },
            Duration::from_secs(30),
        );
        let err = match result {
            Err(err) => err,
            Ok(_) => panic!("expected an error for unknown model"),
        };
        assert!(err.to_string().contains("unsupported embedding model"));
    }

    #[test]
    fn init_timeout_returns_descriptive_error() {
        let start = std::time::Instant::now();
        let result = run_with_init_timeout(Duration::from_millis(100), || {
            std::thread::sleep(Duration::from_secs(30));
            Ok(noop_embedder(8))
        });
        let err = match result {
            Err(err) => err,
            Ok(_) => panic!("expected the timeout to fire"),
        };
        assert!(err.to_string().contains("timed out"), "{err}");
        // Must fail long before the closure's own 30s sleep completes.
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn init_worker_panic_reports_disconnect() {
        let result = run_with_init_timeout(Duration::from_secs(5), || {
            panic!("embedder thread crashed");
        });
        let err = match result {
            Err(err) => err,
            Ok(_) => panic!("expected the panicking worker to surface an error"),
        };
        assert!(err.to_string().contains("unexpectedly"), "{err}");
    }
}

//! Mermaid fence handling: deferred IR always; diagram render behind `feature = "mermaid"`.

use std::sync::Arc;

#[cfg(feature = "mermaid")]
const MIN_CACHE_WIDTH: u16 = 16;
#[cfg(feature = "mermaid")]
const MAX_SOURCE_BYTES: usize = 32 * 1024;

/// Shared diagram (or source fallback) for a fence at `inner` columns.
///
/// Measure and paint must call this so they see the **same** string in one frame,
/// including clipped fallbacks. Errors (invalid / too-narrow / oversize) return the
/// raw source and are not cached.
pub fn mermaid_display_shared(source: &str, inner: u16) -> Arc<str> {
    #[cfg(feature = "mermaid")]
    {
        render::display_shared(source, inner)
    }
    #[cfg(not(feature = "mermaid"))]
    {
        let _ = inner;
        Arc::<str>::from(source)
    }
}

/// Owned copy of [`mermaid_display_shared`].
pub fn mermaid_display_text(source: &str, inner: u16) -> String {
    mermaid_display_shared(source, inner).to_string()
}

#[cfg(feature = "mermaid")]
mod render {
    use std::collections::{HashMap, VecDeque};
    use std::hash::{Hash, Hasher};
    use std::sync::{Arc, Mutex};

    use super::{MAX_SOURCE_BYTES, MIN_CACHE_WIDTH, clip_diagram};

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct MermaidCacheKey {
        source_hash: u64,
        max_width: u16,
    }

    struct MermaidCache {
        map: HashMap<MermaidCacheKey, Arc<str>>,
        lru: VecDeque<MermaidCacheKey>,
        bytes: usize,
    }

    impl MermaidCache {
        fn new() -> Self {
            Self {
                map: HashMap::with_capacity(MAX_ENTRIES),
                lru: VecDeque::with_capacity(MAX_ENTRIES),
                bytes: 0,
            }
        }

        fn get(&mut self, key: &MermaidCacheKey) -> Option<Arc<str>> {
            let val = self.map.get(key)?.clone();
            if let Some(i) = self.lru.iter().position(|k| k == key) {
                self.lru.remove(i);
                self.lru.push_back(*key);
            }
            Some(val)
        }

        fn insert(&mut self, key: MermaidCacheKey, val: Arc<str>) {
            if val.len() > MAX_ENTRY_BYTES {
                return;
            }
            if let Some(old) = self.map.remove(&key) {
                self.bytes = self.bytes.saturating_sub(old.len());
                self.lru.retain(|k| k != &key);
            }
            while !self.map.is_empty() && (self.map.len() >= MAX_ENTRIES || self.bytes + val.len() > MAX_BYTES) {
                let Some(old_key) = self.lru.pop_front() else {
                    break;
                };
                if let Some(old) = self.map.remove(&old_key) {
                    self.bytes = self.bytes.saturating_sub(old.len());
                }
            }
            self.bytes = self.bytes.saturating_add(val.len());
            self.map.insert(key, val);
            self.lru.push_back(key);
        }
    }

    const MAX_ENTRIES: usize = 16;
    const MAX_BYTES: usize = 256 * 1024;
    const MAX_ENTRY_BYTES: usize = 32 * 1024;

    static MERMAID_RENDER_CACHE: std::sync::OnceLock<Mutex<MermaidCache>> = std::sync::OnceLock::new();

    fn mermaid_cache() -> &'static Mutex<MermaidCache> {
        MERMAID_RENDER_CACHE.get_or_init(|| Mutex::new(MermaidCache::new()))
    }

    fn source_hash(source: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        hasher.finish()
    }

    /// Render a mermaid diagram to Unicode/ASCII box-drawing text.
    ///
    /// At most two mermaid-text calls (strict Unicode, then strict ASCII). Each call already
    /// runs progressive compaction internally — do not stack four full pipelines.
    ///
    /// Successful renders (exact or clipped-to-width) are cached per `(source, width)` so
    /// measure and paint share one `Arc<str>`. Errors are not cached.
    pub fn render_mermaid_at_width(source: &str, max_width: u16) -> Result<String, mermaid_text::Error> {
        render_shared(source, max_width).map(|text| text.to_string())
    }

    pub(super) fn display_shared(source: &str, inner: u16) -> Arc<str> {
        render_shared(source, inner).unwrap_or_else(|err| {
            log::warn!("mermaid render failed: {err}");
            Arc::<str>::from(source)
        })
    }

    fn render_shared(source: &str, max_width: u16) -> Result<Arc<str>, mermaid_text::Error> {
        if source.trim().is_empty() {
            return Err(mermaid_text::Error::EmptyInput);
        }
        if source.len() > MAX_SOURCE_BYTES {
            return Err(mermaid_text::Error::ParseError("mermaid source exceeds render budget".into()));
        }
        let max_width = max_width.max(1);

        let cache_key = MermaidCacheKey {
            source_hash: source_hash(source),
            max_width,
        };

        if max_width >= MIN_CACHE_WIDTH
            && let Ok(mut cache) = mermaid_cache().lock()
            && let Some(cached) = cache.get(&cache_key)
        {
            return Ok(cached);
        }

        let output = render_mermaid_uncached(source, max_width as usize)?;
        let shared = Arc::<str>::from(output);
        if max_width >= MIN_CACHE_WIDTH
            && let Ok(mut cache) = mermaid_cache().lock()
        {
            cache.insert(cache_key, Arc::clone(&shared));
        }
        Ok(shared)
    }

    fn render_mermaid_uncached(source: &str, max_width: usize) -> Result<String, mermaid_text::Error> {
        // One layout pass: mermaid-text's documented compact preset. `gaps_override`
        // skips default (6,2) + three more compact retries (the memory spike).
        // (1,0) overlaps nodes; (2,1) stays readable.
        let opts = mermaid_text::RenderOptions {
            max_width: Some(max_width),
            max_width_strict: false,
            ascii: false,
            color: false,
            gaps_override: Some((2, 1)),
            ..Default::default()
        };
        let output = mermaid_text::render_with_options(source, &opts)?;
        Ok(clip_diagram(&output, max_width))
    }

    #[cfg(test)]
    pub(super) fn cache_len() -> usize {
        mermaid_cache().lock().map(|c| c.map.len()).unwrap_or(0)
    }

    #[cfg(test)]
    pub(super) fn cache_clear() {
        if let Ok(mut cache) = mermaid_cache().lock() {
            *cache = MermaidCache::new();
        }
    }
}

#[cfg(feature = "mermaid")]
pub use render::render_mermaid_at_width;

/// Clip each line to `max_width` display columns (prefix only — no ellipsis).
/// Used so paint measure cannot expand past the mermaid card.
#[cfg(feature = "mermaid")]
fn clip_diagram(output: &str, max_width: usize) -> String {
    output
        .lines()
        .map(|line| clip_to_width(line, max_width))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(feature = "mermaid")]
fn clip_to_width(text: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max_width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

#[cfg(all(test, feature = "mermaid"))]
mod tests {
    use super::*;

    #[test]
    fn mermaid_render_at_width_produces_diagram() {
        render::cache_clear();
        let src = "graph LR; A[Build] --> B[Deploy]";
        let output = render_mermaid_at_width(src, 120).expect("valid mermaid renders");
        assert!(output.contains("Build"), "diagram contains 'Build'");
        assert!(output.contains("Deploy"), "diagram contains 'Deploy'");
    }

    #[test]
    fn mermaid_render_never_reverts_on_overflow() {
        render::cache_clear();
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy] --> D[Verify] --> E[Release]";
        let output = render_mermaid_at_width(src, 40).expect("valid diagram must render");
        assert!(!output.trim_start().starts_with("graph "));
        for line in output.lines() {
            assert!(unicode_width::UnicodeWidthStr::width(line) <= 40, "line exceeds card: {line:?}");
        }
    }

    #[test]
    fn mermaid_render_compacts_to_fit_wide_diagram() {
        render::cache_clear();
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy]";
        let output = render_mermaid_at_width(src, 80).expect("fits within 80 cols");
        assert!(output.contains("Build"));
        assert!(output.contains("Deploy"));
    }

    #[test]
    fn mermaid_render_stays_inside_card_when_narrow() {
        render::cache_clear();
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy] --> D[Verify] --> E[Release]";
        let output = render_mermaid_at_width(src, 24).expect("renders");
        assert!(!output.trim_start().starts_with("graph "));
        for line in output.lines() {
            assert!(unicode_width::UnicodeWidthStr::width(line) <= 24);
        }
    }

    #[test]
    fn mermaid_render_returns_error_for_invalid_source() {
        let src = "this is not valid mermaid {{{";
        let result = render_mermaid_at_width(src, 80);
        assert!(result.is_err(), "invalid mermaid returns Err so caller can fallback");
    }

    #[test]
    fn mermaid_render_caches_exact_hits_only() {
        render::cache_clear();
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy]";
        let first = render_mermaid_at_width(src, 80).expect("renders");
        let second = render_mermaid_at_width(src, 80).expect("renders");
        assert_eq!(first, second);
        assert!(render::cache_len() >= 1);

        let before = render::cache_len();
        let _again = render_mermaid_at_width(src, 80).expect("cached");
        assert_eq!(render::cache_len(), before, "same key must not add a cache entry");
        let _narrow = render_mermaid_at_width(src, 16).expect("narrow");
        assert!(render::cache_len() >= before);
    }

    #[test]
    fn mermaid_renders_on_narrow_terminal() {
        render::cache_clear();
        let src = "graph LR; A[Build] --> B[Deploy]";
        let output = render_mermaid_at_width(src, 20).expect("narrow terminal still diagrams");
        assert!(
            output.contains("Build") || output.contains('─') || output.contains('-'),
            "narrow output should stay a diagram, got {output:?}"
        );
        assert!(!output.trim_start().starts_with("graph "));
    }

    #[test]
    fn mermaid_display_shared_is_not_raw_source_for_valid_graph() {
        render::cache_clear();
        let src = "graph TD\n    A[Start] --> B[End]";
        let shown = mermaid_display_shared(src, 48);
        assert_ne!(&*shown, src, "valid mermaid must not fall back to fence source");
        assert!(shown.contains("Start") || shown.contains('─') || shown.contains('-'));
    }
}

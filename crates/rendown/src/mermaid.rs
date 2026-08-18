//! Mermaid fence handling: deferred IR always; diagram render behind `feature = "mermaid"`.

/// Skip mermaid-text for implausibly narrow measure passes (layout flicker).
#[cfg(feature = "mermaid")]
const MIN_RENDER_WIDTH: u16 = 12;
/// Do not pin first-paint / inset-settling widths in the cache.
#[cfg(feature = "mermaid")]
const MIN_CACHE_WIDTH: u16 = 24;
/// Refuse to parse huge fences (mermaid-text graphs are the memory hog).
#[cfg(feature = "mermaid")]
const MAX_SOURCE_BYTES: usize = 8 * 1024;

/// Text to paint for a deferred mermaid fence at `inner` columns.
///
/// With `feature = "mermaid"`, this is a box-drawing diagram (or the source on error).
/// Without the feature, this is the raw mermaid source.
pub fn mermaid_display_text(source: &str, inner: u16) -> String {
    #[cfg(feature = "mermaid")]
    {
        render_mermaid_at_width(source, inner).unwrap_or_else(|_| source.to_string())
    }
    #[cfg(not(feature = "mermaid"))]
    {
        let _ = inner;
        source.to_string()
    }
}

#[cfg(feature = "mermaid")]
mod render {
    use std::collections::{HashMap, VecDeque};
    use std::hash::{Hash, Hasher};
    use std::sync::{Arc, Mutex};

    use super::{MAX_SOURCE_BYTES, MIN_CACHE_WIDTH, MIN_RENDER_WIDTH, truncate_diagram_lines};

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

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Quality {
        Exact,
        Clipped,
    }

    /// Render a mermaid diagram to Unicode/ASCII box-drawing text.
    ///
    /// At most two mermaid-text calls (strict Unicode, then strict ASCII). Each call already
    /// runs progressive compaction internally — do not stack four full pipelines.
    ///
    /// Only **exact** (strict-width) results are cached. Truncated fallbacks and errors are
    /// never stored, so a bad first frame cannot pin clipped output until process restart.
    pub fn render_mermaid_at_width(source: &str, max_width: u16) -> Result<String, mermaid_text::Error> {
        if source.trim().is_empty() {
            return Err(mermaid_text::Error::EmptyInput);
        }
        if source.len() > MAX_SOURCE_BYTES {
            return Err(mermaid_text::Error::ParseError("mermaid source exceeds render budget".into()));
        }
        if max_width < MIN_RENDER_WIDTH {
            return Err(mermaid_text::Error::TooWide {
                requested: max_width as usize,
                actual: MIN_RENDER_WIDTH as usize,
            });
        }

        let max_width_usize = max_width as usize;
        let cache_key = MermaidCacheKey {
            source_hash: source_hash(source),
            max_width,
        };

        if max_width >= MIN_CACHE_WIDTH
            && let Ok(mut cache) = mermaid_cache().lock()
            && let Some(cached) = cache.get(&cache_key)
        {
            return Ok(cached.to_string());
        }

        let (output, quality) = render_mermaid_uncached(source, max_width_usize)?;

        if quality == Quality::Exact
            && max_width >= MIN_CACHE_WIDTH
            && let Ok(mut cache) = mermaid_cache().lock()
        {
            cache.insert(cache_key, Arc::<str>::from(output.as_str()));
        }

        Ok(output)
    }

    fn render_mermaid_uncached(source: &str, max_width: usize) -> Result<(String, Quality), mermaid_text::Error> {
        let strict_unicode = mermaid_text::RenderOptions {
            max_width: Some(max_width),
            max_width_strict: true,
            ascii: false,
            color: false,
            ..Default::default()
        };
        match mermaid_text::render_with_options(source, &strict_unicode) {
            Ok(output) => return Ok((output, Quality::Exact)),
            Err(mermaid_text::Error::TooWide { .. }) => {}
            Err(err) => return Err(err),
        }

        let strict_ascii = mermaid_text::RenderOptions {
            max_width: Some(max_width),
            max_width_strict: true,
            ascii: true,
            color: false,
            ..Default::default()
        };
        match mermaid_text::render_with_options(source, &strict_ascii) {
            Ok(output) => return Ok((output, Quality::Exact)),
            Err(mermaid_text::Error::TooWide { .. }) => {}
            Err(err) => return Err(err),
        }

        let soft = mermaid_text::RenderOptions {
            max_width: Some(max_width),
            max_width_strict: false,
            ascii: false,
            color: false,
            ..Default::default()
        };
        let output = mermaid_text::render_with_options(source, &soft)?;
        Ok((truncate_diagram_lines(&output, max_width), Quality::Clipped))
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

#[cfg(feature = "mermaid")]
fn truncate_diagram_lines(output: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    use crate::wrap::truncate_with_ellipsis;

    output
        .lines()
        .map(|line| {
            if line.width() > max_width {
                truncate_with_ellipsis(line, max_width)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        let output = render_mermaid_at_width(src, 20).expect("valid diagram must render");
        for line in output.lines() {
            assert!(
                unicode_width::UnicodeWidthStr::width(line) <= 20,
                "line exceeds width: {line:?}"
            );
        }
        assert!(output.contains('─') || output.contains('-') || output.contains("Build"));
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
    fn mermaid_render_truncates_overflowing_lines() {
        render::cache_clear();
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy] --> D[Verify] --> E[Release]";
        let output = render_mermaid_at_width(src, 16).expect("renders");
        for line in output.lines() {
            assert!(
                unicode_width::UnicodeWidthStr::width(line) <= 16,
                "overflowing line not truncated: {line:?}"
            );
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
        let _narrow = render_mermaid_at_width(src, 16);
        assert_eq!(
            render::cache_len(),
            before,
            "clipped / sub-cache-width renders must not grow the cache"
        );
    }

    #[test]
    fn mermaid_refuses_tiny_width_without_caching() {
        render::cache_clear();
        let src = "graph LR; A[Build] --> B[Deploy]";
        let result = render_mermaid_at_width(src, 8);
        assert!(result.is_err());
        assert_eq!(render::cache_len(), 0);
    }
}

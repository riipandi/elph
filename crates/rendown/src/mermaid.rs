//! Mermaid fence handling: deferred IR always; diagram render behind `feature = "mermaid"`.

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
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::Mutex;

    use super::truncate_diagram_lines;

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct MermaidCacheKey {
        source_hash: u64,
        max_width: u16,
    }

    static MERMAID_RENDER_CACHE: std::sync::OnceLock<Mutex<HashMap<MermaidCacheKey, String>>> =
        std::sync::OnceLock::new();

    fn mermaid_cache() -> &'static Mutex<HashMap<MermaidCacheKey, String>> {
        MERMAID_RENDER_CACHE.get_or_init(|| Mutex::new(HashMap::with_capacity(64)))
    }

    const MERMAID_CACHE_MAX_ENTRIES: usize = 128;

    /// Render a mermaid diagram to Unicode/ASCII box-drawing text, compacted to `max_width`.
    ///
    /// Cached per `(source, width)`. Invalid source returns `Err` so callers can fall back.
    pub fn render_mermaid_at_width(source: &str, max_width: u16) -> Result<String, mermaid_text::Error> {
        let max_width_usize = max_width.max(1) as usize;
        let source_hash = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            source.hash(&mut hasher);
            hasher.finish()
        };
        let cache_key = MermaidCacheKey { source_hash, max_width };
        if let Ok(cache) = mermaid_cache().lock()
            && let Some(cached) = cache.get(&cache_key)
        {
            return Ok(cached.clone());
        }

        let output = render_mermaid_uncached(source, max_width_usize);

        if let Ok(ref rendered) = output
            && let Ok(mut cache) = mermaid_cache().lock()
        {
            if cache.len() >= MERMAID_CACHE_MAX_ENTRIES {
                let to_remove = cache.len() / 2;
                let keys: Vec<_> = cache.keys().take(to_remove).copied().collect();
                for key in keys {
                    cache.remove(&key);
                }
            }
            cache.insert(cache_key, rendered.clone());
        }

        output
    }

    fn render_mermaid_uncached(source: &str, max_width: usize) -> Result<String, mermaid_text::Error> {
        let strict_unicode = mermaid_text::RenderOptions {
            max_width: Some(max_width),
            max_width_strict: true,
            ascii: false,
            color: false,
            ..Default::default()
        };
        if let Ok(output) = mermaid_text::render_with_options(source, &strict_unicode) {
            return Ok(output);
        }

        let strict_ascii = mermaid_text::RenderOptions {
            max_width: Some(max_width),
            max_width_strict: true,
            ascii: true,
            color: false,
            ..Default::default()
        };
        if let Ok(output) = mermaid_text::render_with_options(source, &strict_ascii) {
            return Ok(output);
        }

        let soft_unicode = mermaid_text::RenderOptions {
            max_width: Some(max_width),
            max_width_strict: false,
            ascii: false,
            color: false,
            ..Default::default()
        };
        if let Ok(output) = mermaid_text::render_with_options(source, &soft_unicode) {
            return Ok(truncate_diagram_lines(&output, max_width));
        }

        let soft_ascii = mermaid_text::RenderOptions {
            max_width: Some(max_width),
            max_width_strict: false,
            ascii: true,
            color: false,
            ..Default::default()
        };
        mermaid_text::render_with_options(source, &soft_ascii).map(|output| truncate_diagram_lines(&output, max_width))
    }

    #[cfg(test)]
    pub(super) fn cache_len() -> usize {
        mermaid_cache().lock().map(|c| c.len()).unwrap_or(0)
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
        let src = "graph LR; A[Build] --> B[Deploy]";
        let output = render_mermaid_at_width(src, 120).expect("valid mermaid renders");
        assert!(output.contains("Build"), "diagram contains 'Build'");
        assert!(output.contains("Deploy"), "diagram contains 'Deploy'");
    }

    #[test]
    fn mermaid_render_never_reverts_on_overflow() {
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy] --> D[Verify] --> E[Release]";
        let output = render_mermaid_at_width(src, 20).expect("valid diagram must render");
        for line in output.lines() {
            assert!(
                line.chars().count() <= 20,
                "line exceeds width: {line:?} ({} chars)",
                line.chars().count()
            );
        }
        assert!(output.contains('─') || output.contains('-'), "diagram keeps its edges");
    }

    #[test]
    fn mermaid_render_compacts_to_fit_wide_diagram() {
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy]";
        let output = render_mermaid_at_width(src, 80).expect("fits within 80 cols");
        assert!(output.contains("Build"));
        assert!(output.contains("Deploy"));
    }

    #[test]
    fn mermaid_render_truncates_overflowing_lines() {
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy] --> D[Verify] --> E[Release]";
        let output = render_mermaid_at_width(src, 16).expect("renders");
        for line in output.lines() {
            assert!(line.chars().count() <= 16, "overflowing line not truncated: {line:?}");
        }
    }

    #[test]
    fn mermaid_render_returns_error_for_invalid_source() {
        let src = "this is not valid mermaid {{{";
        let result = render_mermaid_at_width(src, 80);
        assert!(result.is_err(), "invalid mermaid returns Err so caller can fallback");
    }

    #[test]
    fn mermaid_render_caches_per_source_and_width() {
        let src = "graph LR; A[Build] --> B[Test] --> C[Deploy]";
        let first = render_mermaid_at_width(src, 80).expect("renders");
        let second = render_mermaid_at_width(src, 80).expect("renders");
        assert_eq!(first, second);
        let _wide = render_mermaid_at_width(src, 120).expect("renders");
        let _narrow = render_mermaid_at_width(src, 40).expect("renders");
        assert!(
            render::cache_len() >= 2,
            "cache should have entries for different (source, width) pairs, got {}",
            render::cache_len()
        );
    }
}

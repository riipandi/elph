//! Incremental markdown cache for one streaming assistant message.

use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use elph_tui::MarkdownDocument;
use elph_tui::markdown_document_row_count;

use super::layout::markdown_part_row_count;
use super::partition::find_stable_boundary;

/// One stable markdown segment with optional parsed document cache.
#[derive(Clone)]
pub struct RenderedPart {
    pub source_end: usize,
    pub source_hash: u64,
    pub row_count: u16,
    pub document: Option<MarkdownDocument>,
}

/// Streaming markdown state for [`crate::tui::transcript::TranscriptMessage`].
pub struct AssistantMarkdownBuffer {
    pub stable_end: usize,
    pub parts: Vec<RenderedPart>,
    pub wrap_width: u16,
    pub stream_complete: bool,
    /// Cached built document (stable + tail merged) to avoid re-parsing the streaming
    /// tail on every layout+paint pass within the same frame.
    built_doc_cache: Mutex<BuiltDocCache>,
}

impl Clone for AssistantMarkdownBuffer {
    fn clone(&self) -> Self {
        Self {
            stable_end: self.stable_end,
            parts: self.parts.clone(),
            wrap_width: self.wrap_width,
            stream_complete: self.stream_complete,
            built_doc_cache: Mutex::new((*self.built_doc_cache.lock().unwrap()).clone()),
        }
    }
}

impl Default for AssistantMarkdownBuffer {
    fn default() -> Self {
        Self {
            stable_end: 0,
            parts: Vec::new(),
            wrap_width: 0,
            stream_complete: false,
            built_doc_cache: Mutex::new(BuiltDocCache::default()),
        }
    }
}

/// Per-buffer cache for the merged (stable + tail) markdown document.
///
/// Layout (`assistant_row_count`) and paint (`render_markdown_buffer`) both build
/// the same merged document each frame. This cache lets them share the result so
/// the streaming-tail parse runs once per frame instead of twice.
#[derive(Clone, Default)]
struct BuiltDocCache {
    key: Option<u64>, // tail_hash
    doc: Option<MarkdownDocument>,
}

pub fn stable_source_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

impl AssistantMarkdownBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute (or return cached) the merged markdown document for this buffer.
    ///
    /// Layout and paint both call this each frame. The cache is keyed by tail hash —
    /// when the tail content changes the cached doc is discarded. This eliminates
    /// the duplicate streaming-tail parse that previously happened once in
    /// `assistant_row_count` (layout) and again in `render_markdown_buffer` (paint)
    /// per visible assistant message per frame.
    pub fn built_document(&self, raw: &str, tail_foreground: iocraft::prelude::Color) -> MarkdownDocument {
        let tail = self.tail(raw);
        let tail_hash = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            tail.hash(&mut h);
            h.finish()
        };
        // INVARIANT: no panicking code runs while holding this lock.
        let mut cache = self.built_doc_cache.lock().unwrap();
        if cache.key == Some(tail_hash) {
            // INVARIANT: key match guarantees doc is Some — we set both atomically.
            return cache.doc.clone().expect("cached doc must be present");
        }
        let doc = super::render::build_assistant_markdown_document(self, raw, tail_foreground);
        cache.key = Some(tail_hash);
        cache.doc = Some(doc.clone());
        doc
    }

    pub fn tail<'a>(&self, raw: &'a str) -> &'a str {
        raw.get(self.stable_end..).unwrap_or("")
    }

    pub fn has_rendered_body(&self) -> bool {
        !self.parts.is_empty() || self.stable_end > 0
    }

    pub fn needs_parse(&self) -> bool {
        self.parts
            .iter()
            .any(|part| part.document.is_none() && part.source_end > 0)
    }

    /// Advance stable boundary (cheap — no parsing).
    ///
    /// Returns `true` when `parts` or `stable_end` changed.
    pub fn refresh_stable(&mut self, raw: &str, wrap_width: u16) -> bool {
        if wrap_width == 0 {
            return false;
        }
        if self.wrap_width != wrap_width && self.has_rendered_body() {
            self.stable_end = 0;
            self.parts.clear();
        }
        self.wrap_width = wrap_width;

        let force = self.stream_complete;
        let mut new_end = find_stable_boundary(raw, force);
        // Clamp to a char boundary so multi-byte stream text never panics on slice.
        if new_end > raw.len() {
            new_end = raw.len();
        } else if new_end < raw.len() && !raw.is_char_boundary(new_end) {
            new_end = raw.floor_char_boundary(new_end);
        }
        if new_end <= self.stable_end {
            return false;
        }

        let Some(stable) = raw.get(..new_end) else {
            return false;
        };
        let hash = stable_source_hash(stable);
        let preserved_doc = self
            .parts
            .first()
            .filter(|part| part.source_hash == hash)
            .and_then(|part| part.document.clone());

        let row_count = preserved_doc
            .as_ref()
            .map(|doc| markdown_document_row_count(doc, wrap_width))
            .unwrap_or_else(|| markdown_part_row_count(stable, wrap_width));

        self.parts = vec![RenderedPart {
            source_end: new_end,
            source_hash: hash,
            row_count,
            document: preserved_doc,
        }];
        self.stable_end = new_end;
        true
    }

    pub fn apply_document(&mut self, expected_hash: u64, document: MarkdownDocument) -> bool {
        let Some(part) = self.parts.first_mut() else {
            return false;
        };
        if part.source_hash != expected_hash {
            return false;
        }
        part.row_count = markdown_document_row_count(&document, self.wrap_width);
        part.document = Some(document);
        true
    }

    pub fn mark_stream_complete(&mut self) {
        self.stream_complete = true;
    }

    /// Drop cached parsed documents to free memory while keeping streaming state.
    ///
    /// The retained `stable_end`, `stream_complete`, `wrap_width`, and row counts let the
    /// layout path continue to measure correctly; the worker re-parses the document when the
    /// row scrolls back into the parse window (the stable source is still in the message
    /// content). Used by retention to shed memory from off-screen assistant messages.
    pub fn drop_cached_documents(&mut self) {
        for part in &mut self.parts {
            part.document = None;
        }
    }

    /// True when any stable part still holds a parsed document.
    pub fn has_cached_documents(&self) -> bool {
        self.parts.iter().any(|part| part.document.is_some())
    }

    /// Copy of this buffer with every parsed document released.
    ///
    /// Retention runs on `Arc`-shared buffers: `Arc::make_mut` would deep-clone every cached
    /// `MarkdownDocument` (exactly the memory being reclaimed) before dropping it. Building a
    /// document-free copy skips that spike.
    pub fn without_documents(&self) -> Self {
        Self {
            stable_end: self.stable_end,
            wrap_width: self.wrap_width,
            stream_complete: self.stream_complete,
            parts: self
                .parts
                .iter()
                .map(|part| RenderedPart {
                    source_end: part.source_end,
                    source_hash: part.source_hash,
                    row_count: part.row_count,
                    document: None,
                })
                .collect(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_grows_stable_prefix() {
        let mut buf = AssistantMarkdownBuffer::new();
        let raw = "# Hi\n\nParagraph.";
        assert!(buf.refresh_stable(raw, 40));
        assert_eq!(buf.stable_end, 6);
        buf.mark_stream_complete();
        assert!(buf.refresh_stable(raw, 40));
        assert_eq!(buf.stable_end, raw.len());
        assert_eq!(buf.parts.len(), 1);
        assert!(buf.parts[0].row_count > 0);
    }

    #[test]
    fn refresh_skips_when_boundary_unchanged() {
        let mut buf = AssistantMarkdownBuffer::new();
        let raw = "no paragraph break yet";
        assert!(!buf.refresh_stable(raw, 40));
        assert_eq!(buf.stable_end, 0);
        assert!(buf.parts.is_empty());
    }

    #[test]
    fn width_change_invalidates_cache() {
        let mut buf = AssistantMarkdownBuffer::new();
        let raw = "A\n\nB";
        assert!(buf.refresh_stable(raw, 40));
        assert!(buf.refresh_stable(raw, 30));
        assert_eq!(buf.wrap_width, 30);
    }

    #[test]
    fn apply_document_updates_row_count() {
        let mut buf = AssistantMarkdownBuffer::new();
        let raw = "Hello **world**";
        buf.mark_stream_complete();
        assert!(buf.refresh_stable(raw, 40));
        let hash = buf.parts[0].source_hash;
        let doc = elph_tui::parse_markdown_document(raw);
        assert!(buf.apply_document(hash, doc));
        assert!(buf.parts[0].document.is_some());
    }

    #[test]
    fn drop_cached_documents_frees_document_keeps_metadata() {
        let mut buf = AssistantMarkdownBuffer::new();
        let raw = "## Plan\n\nA paragraph that wraps across the width nicely.\n\n| Col | Val |\n| --- | --- |\n| a | 1 |\n| b | 2 |\n\nLast paragraph.\n";
        buf.mark_stream_complete();
        assert!(buf.refresh_stable(raw, 80));
        let doc = elph_tui::parse_markdown_document(raw);
        let hash = buf.parts[0].source_hash;
        assert!(buf.apply_document(hash, doc));
        assert!(buf.parts[0].document.is_some());
        let row_count_before = buf.parts[0].row_count;
        assert!(row_count_before > 0);

        // Drop the cached document — simulates what retention does for off-screen messages.
        buf.drop_cached_documents();
        assert!(buf.parts[0].document.is_none(), "document must be freed");
        // Streaming metadata is preserved so layout still measures correctly.
        assert_eq!(buf.stable_end, raw.len());
        assert!(buf.stream_complete);
        assert_eq!(buf.wrap_width, 80);
        assert_eq!(buf.parts[0].row_count, row_count_before);
        assert_eq!(buf.parts[0].source_hash, hash);
    }

    #[test]
    fn built_document_caches_and_reuses() {
        let mut buf = AssistantMarkdownBuffer::new();
        let raw = "Para1.\n\n```mermaid\ngraph LR; A --> B\n```\n";
        buf.mark_stream_complete();
        buf.refresh_stable(raw, 80);

        // First call builds the document.
        let doc1 = buf.built_document(raw, iocraft::prelude::Color::Reset);
        assert!(!doc1.lines.is_empty());

        // Second call with same tail returns cached copy.
        let doc2 = buf.built_document(raw, iocraft::prelude::Color::Reset);
        assert_eq!(doc1.lines.len(), doc2.lines.len());

        // Different tail invalidates cache. Use a completely different buffer to avoid
        // stable_end drift from the first refresh.
        let mut buf2 = AssistantMarkdownBuffer::new();
        let raw2 = "Solo paragraph.";
        buf2.mark_stream_complete();
        buf2.refresh_stable(raw2, 80);
        let doc3 = buf2.built_document(raw2, iocraft::prelude::Color::Reset);
        assert_ne!(doc1.lines.len(), doc3.lines.len());
    }
}

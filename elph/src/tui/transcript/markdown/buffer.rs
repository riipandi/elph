//! Incremental markdown cache for one streaming assistant message.

use std::hash::{Hash, Hasher};

use elph_tui::{MarkdownDocument, markdown_document_row_count};

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
#[derive(Clone, Default)]
pub struct AssistantMarkdownBuffer {
    pub stable_end: usize,
    pub parts: Vec<RenderedPart>,
    pub wrap_width: u16,
    pub stream_complete: bool,
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
        let new_end = find_stable_boundary(raw, force);
        if new_end <= self.stable_end {
            return false;
        }

        let stable = &raw[..new_end];
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
}

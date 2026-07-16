//! Cloneable markdown document model for cache + background parsing.

use iocraft::prelude::{Color, Weight};

/// Semantic role of a rendered markdown line (drives spacing and layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownLineKind {
    Paragraph,
    /// Hard-wrapped line within the same block (no extra gap after).
    Continuation,
    Heading(u8),
    ListItem,
    Code,
    Blockquote,
    Rule,
    Blank,
}

/// One styled text run inside a line.
#[derive(Clone, Debug, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub color: Color,
    pub weight: Weight,
    pub italic: bool,
}

impl StyledSpan {
    pub fn plain(text: impl Into<String>, color: Color) -> Self {
        Self {
            text: text.into(),
            color,
            weight: Weight::Normal,
            italic: false,
        }
    }
}

/// One renderable line (paragraph, heading, list item, or code line).
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownLine {
    pub kind: MarkdownLineKind,
    pub spans: Vec<StyledSpan>,
    /// Code blocks use a subtle tinted background in the transcript.
    pub code_background: bool,
}

impl MarkdownLine {
    pub fn blank() -> Self {
        Self {
            kind: MarkdownLineKind::Blank,
            spans: Vec::new(),
            code_background: false,
        }
    }

    pub fn is_blank(&self) -> bool {
        self.kind == MarkdownLineKind::Blank || self.spans.iter().all(|span| span.text.trim().is_empty())
    }
}

/// Parsed markdown document (safe to cache and parse off the UI thread).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkdownDocument {
    pub lines: Vec<MarkdownLine>,
}

impl MarkdownDocument {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() || self.lines.iter().all(|line| line.is_blank())
    }

    /// Collapse duplicate blank lines and trim trailing empties.
    pub fn normalize(mut self) -> Self {
        let mut lines = Vec::with_capacity(self.lines.len());
        for line in self.lines.drain(..) {
            if line.is_blank() && lines.last().is_some_and(|last: &MarkdownLine| last.is_blank()) {
                continue;
            }
            lines.push(line);
        }
        while lines.last().is_some_and(|line: &MarkdownLine| line.is_blank()) {
            lines.pop();
        }
        Self { lines }
    }
}

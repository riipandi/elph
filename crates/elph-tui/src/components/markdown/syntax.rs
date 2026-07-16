//! Syntax highlighting via syntect and the two-face extended syntax set.

use std::io::Cursor;
use std::path::Path;
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme as SyntectTheme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

static SYNTAX_HIGHLIGHTER: OnceLock<SyntaxHighlighter> = OnceLock::new();

/// Shared syntect configuration (tokyo-night theme + two-face syntax set).
pub struct SyntaxHighlighter {
    pub theme: SyntectTheme,
    pub syntax_set: SyntaxSet,
}

impl SyntaxHighlighter {
    pub fn global() -> &'static Self {
        SYNTAX_HIGHLIGHTER.get_or_init(Self::build)
    }

    fn build() -> Self {
        let theme_bytes = include_bytes!("../../../assets/tokyo-night.tmTheme");
        let mut cursor = Cursor::new(theme_bytes.as_slice());
        let theme = ThemeSet::load_from_reader(&mut cursor).expect("tokyo-night.tmTheme must load");
        let syntax_set = two_face::syntax::extra_newlines();
        Self { theme, syntax_set }
    }

    pub fn find_syntax_by_token(&self, token: &str) -> Option<&SyntaxReference> {
        self.syntax_set.find_syntax_by_token(token)
    }

    pub fn find_syntax_by_file_path(&self, file_path: &Path) -> Option<&SyntaxReference> {
        let ext = file_path.extension()?.to_str()?;
        self.syntax_set.find_syntax_by_extension(ext)
    }

    pub fn highlight_lines_for_fence_info(&self, fence_info: &str) -> Option<HighlightLines<'_>> {
        Some(HighlightLines::new(self.find_syntax_for_fence_info(fence_info)?, &self.theme))
    }

    fn find_syntax_for_fence_info(&self, fence_info: &str) -> Option<&SyntaxReference> {
        if let Some((_, _, path)) = parse_line_citation_fence_info(fence_info)
            && let Some(syntax) = self.find_syntax_by_file_path(Path::new(path))
        {
            return Some(syntax);
        }
        self.find_syntax_by_token(fence_info)
    }
}

fn parse_line_citation_fence_info(info: &str) -> Option<(&str, &str, &str)> {
    let mut parts = info.splitn(3, ':');
    let start = parts.next()?;
    let end = parts.next()?;
    let path = parts.next()?;
    if start.is_empty() || !start.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if end.is_empty() || !end.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if path.is_empty() {
        return None;
    }
    Some((start, end, path))
}

/// Highlight a fenced code block body into raw syntect regions per line.
pub fn syntax_highlight_raw(fence_info: &str, text: &str) -> Option<Vec<Vec<(syntect::highlighting::Style, String)>>> {
    let highlighter = SyntaxHighlighter::global();
    let mut hl = highlighter.highlight_lines_for_fence_info(fence_info)?;
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(text) {
        let highlighted = hl.highlight_line(line, &highlighter.syntax_set).ok()?;
        lines.push(
            highlighted
                .into_iter()
                .map(|(style, segment)| (style, segment.to_string()))
                .collect(),
        );
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_rust_fence_info() {
        let highlighter = SyntaxHighlighter::global();
        assert!(highlighter.highlight_lines_for_fence_info("rust").is_some());
    }

    #[test]
    fn resolves_path_citation_fence_info() {
        let highlighter = SyntaxHighlighter::global();
        assert!(
            highlighter
                .highlight_lines_for_fence_info("1:10:crates/elph-tui/src/lib.rs")
                .is_some()
        );
    }
}

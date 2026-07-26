//! Auto-detect URLs, emails, and filesystem paths in plain text.

use std::path::{Path, PathBuf};

use iocraft::prelude::{Color, Weight};
use linkify::{LinkFinder, LinkKind};

use super::model::StyledSpan;

/// Split plain text into styled spans, coloring detected links and paths (no underline).
///
/// Detected targets store the openable URI in [`StyledSpan::href`] (so abbreviated path
/// labels can still open the original path via OSC 8 / Super+click).
pub fn spans_with_links(text: &str, color: Color, weight: Weight, italic: bool, link_color: Color) -> Vec<StyledSpan> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize, String)> = Vec::new();

    let finder = LinkFinder::new();
    for link in finder.links(text) {
        let candidate = &text[link.start()..link.end()];
        let href = match link.kind() {
            LinkKind::Url if url::Url::parse(candidate).is_ok() => Some(candidate.to_string()),
            LinkKind::Email => Some(format!("mailto:{candidate}")),
            _ => None,
        };
        if let Some(href) = href {
            ranges.push((link.start(), link.end(), href));
        }
    }

    // Filesystem paths (not already covered by URL detection).
    for (start, end, href) in detect_path_ranges(text) {
        if ranges.iter().any(|(s, e, _)| *s < end && start < *e) {
            continue;
        }
        ranges.push((start, end, href));
    }

    ranges.sort_by_key(|(start, _, _)| *start);
    if ranges.is_empty() {
        return vec![StyledSpan {
            text: text.to_string(),
            color,
            weight,
            italic,
            underline: false,
            href: None,
        }];
    }

    let mut spans = Vec::new();
    let mut last_end = 0usize;
    for (start, end, href) in ranges {
        if start > last_end {
            spans.push(StyledSpan {
                text: text[last_end..start].to_string(),
                color,
                weight,
                italic,
                underline: false,
                href: None,
            });
        }
        let label = &text[start..end];
        spans.push(StyledSpan {
            text: label.to_string(),
            color: link_color,
            weight,
            italic,
            underline: false,
            href: Some(href),
        });
        last_end = end;
    }
    if last_end < text.len() {
        spans.push(StyledSpan {
            text: text[last_end..].to_string(),
            color,
            weight,
            italic,
            underline: false,
            href: None,
        });
    }
    spans
}

/// Convert a filesystem path to a `file://` URL for OSC 8 (absolute when possible).
pub fn path_to_file_url(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    if path.starts_with("file://") {
        return Some(path.to_string());
    }
    let expanded = expand_home_prefix(path);
    let absolute = if Path::new(&expanded).is_absolute() {
        PathBuf::from(expanded)
    } else {
        std::env::current_dir().ok()?.join(expanded)
    };
    url::Url::from_file_path(&absolute).ok().map(|u| u.to_string())
}

fn expand_home_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest).to_string_lossy().into_owned();
        }
    }
    if path == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

/// Byte ranges of path-like tokens → `file://` href.
fn detect_path_ranges(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Skip whitespace and punctuation that cannot start a path.
        let ch = text[i..].chars().next().unwrap_or('\0');
        let ch_len = ch.len_utf8();
        if ch.is_whitespace() {
            i += ch_len;
            continue;
        }

        let rest = &text[i..];
        if let Some((token_len, href)) = match_path_token(rest) {
            out.push((i, i + token_len, href));
            i += token_len;
            continue;
        }
        i += ch_len;
    }
    out
}

/// Match one path token at the start of `text`. Returns (byte_len, file_url).
fn match_path_token(text: &str) -> Option<(usize, String)> {
    let end = text
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || matches!(c, ',' | ';' | ')' | ']' | '}' | '"' | '\'' | '`' | '|' | '>'))
        .map(|(idx, _)| idx)
        .unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    let mut token = &text[..end];
    // Strip trailing sentence punctuation.
    while let Some(c) = token.chars().last() {
        if matches!(c, '.' | ':' | '!' | '?') {
            token = &token[..token.len() - c.len_utf8()];
        } else {
            break;
        }
    }
    if token.len() < 2 {
        return None;
    }
    if !looks_like_path(token) {
        return None;
    }
    let href = path_to_file_url(token)?;
    Some((token.len(), href))
}

fn looks_like_path(token: &str) -> bool {
    if token.starts_with("http://") || token.starts_with("https://") || token.starts_with("mailto:") {
        return false;
    }
    // Absolute unix / home-relative
    if token.starts_with('/') || token.starts_with("~/") || token == "~" {
        return true;
    }
    // Relative with directory separators and a file-ish tail
    if token.contains('/') {
        // Avoid matching pure URLs fragments or protocol-looking tokens
        if token.contains("://") {
            return false;
        }
        // Require at least one path segment that looks like a file or known dir
        let has_dot_file = token
            .rsplit('/')
            .next()
            .is_some_and(|name| name.contains('.') || name == "Makefile" || name == "Dockerfile");
        let has_dir = token.matches('/').count() >= 1;
        return has_dir && (has_dot_file || token.starts_with("./") || token.starts_with("../"));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::markdown::MarkdownTheme;

    #[test]
    fn linkifies_url_in_plain_text() {
        let theme = MarkdownTheme::default();
        let spans = spans_with_links("Visit https://elph.space today", theme.body, Weight::Normal, false, theme.link);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].text, "Visit ");
        assert_eq!(spans[0].color, theme.body);
        assert_eq!(spans[1].text, "https://elph.space");
        assert_eq!(spans[1].color, theme.link);
        assert_eq!(spans[1].href.as_deref(), Some("https://elph.space"));
        assert!(!spans[1].underline, "links must not paint underline");
        assert_eq!(spans[2].text, " today");
    }

    #[test]
    fn leaves_text_without_links_unchanged() {
        let theme = MarkdownTheme::default();
        let spans = spans_with_links("no links here", theme.body, Weight::Normal, false, theme.link);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "no links here");
        assert_eq!(spans[0].color, theme.body);
        assert!(spans[0].href.is_none());
    }

    #[test]
    fn linkifies_absolute_unix_path() {
        let theme = MarkdownTheme::default();
        let spans = spans_with_links("open /tmp/demo/file.rs please", theme.body, Weight::Normal, false, theme.link);
        let path_span = spans.iter().find(|s| s.text.contains("file.rs")).expect("path span");
        assert!(!path_span.underline, "paths must not paint underline");
        assert!(
            path_span.href.as_deref().is_some_and(|h| h.starts_with("file://")),
            "href={:?}",
            path_span.href
        );
    }

    #[test]
    fn path_to_file_url_preserves_file_scheme() {
        assert_eq!(
            path_to_file_url("file:///tmp/x"),
            Some("file:///tmp/x".to_string())
        );
    }
}

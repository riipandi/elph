//! Auto-detect URLs, emails, and filesystem paths in plain text.

use std::path::{Path, PathBuf};

use linkify::{LinkFinder, LinkKind};

use crate::model::{FontWeight, RgbColor, StyledSpan};

/// Split plain text into styled spans, coloring detected links and paths (no underline).
pub fn spans_with_links(
    text: &str,
    color: RgbColor,
    weight: FontWeight,
    italic: bool,
    link_color: RgbColor,
) -> Vec<StyledSpan> {
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
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest).to_string_lossy().into_owned();
    }
    if path == "~"
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).to_string_lossy().into_owned();
    }
    path.to_string()
}

fn detect_path_ranges(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
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
    if token.contains("://") {
        return false;
    }
    let last = token.rsplit('/').next().unwrap_or(token);
    let has_file = last.contains('.') || last == "Makefile" || last == "Dockerfile";
    let is_explicit_dir = token.ends_with('/') || token.starts_with("./") || token.starts_with("../");
    if token.starts_with('/') || token.starts_with("~/") || token == "~" {
        return has_file || is_explicit_dir;
    }
    if token.contains('/') {
        return has_file || is_explicit_dir;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::MarkdownTheme;

    #[test]
    fn linkifies_url_in_plain_text() {
        let theme = MarkdownTheme::default();
        let spans = spans_with_links(
            "Visit https://elph.space today",
            theme.body,
            FontWeight::Normal,
            false,
            theme.link,
        );
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].text, "https://elph.space");
        assert_eq!(spans[1].href.as_deref(), Some("https://elph.space"));
    }
}

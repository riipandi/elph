//! Auto-detect URLs and emails in plain text via the [`linkify`] crate.

use iocraft::prelude::{Color, Weight};
use linkify::LinkFinder;

use super::model::StyledSpan;

/// Split plain text into styled spans, coloring detected links.
pub fn spans_with_links(text: &str, color: Color, weight: Weight, italic: bool, link_color: Color) -> Vec<StyledSpan> {
    if text.is_empty() {
        return Vec::new();
    }

    let finder = LinkFinder::new();
    let links: Vec<_> = finder.links(text).collect();
    if links.is_empty() {
        return vec![StyledSpan {
            text: text.to_string(),
            color,
            weight,
            italic,
        }];
    }

    let mut spans = Vec::new();
    let mut last_end = 0usize;
    for link in links {
        if link.start() > last_end {
            spans.push(StyledSpan {
                text: text[last_end..link.start()].to_string(),
                color,
                weight,
                italic,
            });
        }
        let candidate = &text[link.start()..link.end()];
        if url::Url::parse(candidate).is_ok() {
            spans.push(StyledSpan {
                text: candidate.to_string(),
                color: link_color,
                weight,
                italic,
            });
        } else {
            spans.push(StyledSpan {
                text: candidate.to_string(),
                color,
                weight,
                italic,
            });
        }
        last_end = link.end();
    }
    if last_end < text.len() {
        spans.push(StyledSpan {
            text: text[last_end..].to_string(),
            color,
            weight,
            italic,
        });
    }
    spans
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
        assert_eq!(spans[2].text, " today");
    }

    #[test]
    fn leaves_text_without_links_unchanged() {
        let theme = MarkdownTheme::default();
        let spans = spans_with_links("no links here", theme.body, Weight::Normal, false, theme.link);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "no links here");
        assert_eq!(spans[0].color, theme.body);
    }
}

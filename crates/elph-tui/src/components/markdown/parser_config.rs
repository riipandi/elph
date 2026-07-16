//! Shared `pulldown-cmark` configuration for the Elph markdown pipeline.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// Parser options for assistant transcript markdown (GFM + extensions).
pub fn parser_options() -> Options {
    Options::ENABLE_GFM
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_MATH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_TABLES
}

/// Offset event stream with single-tilde strikethrough demoted to literal text.
pub fn offset_events(text: &str) -> impl Iterator<Item = (Event<'_>, Range<usize>)> + '_ {
    DoubleTildeOnlyStrike {
        text,
        events: Parser::new_ext(text, parser_options()).into_offset_iter(),
    }
}

struct DoubleTildeOnlyStrike<'a, I> {
    text: &'a str,
    events: I,
}

fn is_double_tilde_strike(text: &str, range: &Range<usize>) -> bool {
    text.get(range.start..).is_some_and(|slice| slice.starts_with("~~"))
}

fn strike_delim_text<'a>(text: &'a str, range: &Range<usize>, opening: bool) -> (Event<'a>, Range<usize>) {
    let delim = if opening {
        let end = range.start + 1;
        (range.start..end, &text[range.start..end])
    } else {
        let start = range.end - 1;
        (start..range.end, &text[start..range.end])
    };
    (Event::Text(delim.1.into()), delim.0)
}

impl<'a, I> Iterator for DoubleTildeOnlyStrike<'a, I>
where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        let (event, range) = self.events.next()?;
        match &event {
            Event::Start(Tag::Strikethrough) if !is_double_tilde_strike(self.text, &range) => {
                Some(strike_delim_text(self.text, &range, true))
            }
            Event::End(TagEnd::Strikethrough) if !is_double_tilde_strike(self.text, &range) => {
                Some(strike_delim_text(self.text, &range, false))
            }
            _ => Some((event, range)),
        }
    }
}

/// Returns true when an open list or blockquote remains at `end` (not checkpoint-safe).
pub fn has_open_container_at(text: &str, end: usize) -> bool {
    let slice = text.get(..end).unwrap_or(text);
    let mut list_depth = 0i32;
    let mut blockquote_depth = 0i32;
    for (event, _) in offset_events(slice) {
        match event {
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => list_depth -= 1,
            Event::Start(Tag::BlockQuote(_)) => blockquote_depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => blockquote_depth -= 1,
            _ => {}
        }
    }
    list_depth > 0 || blockquote_depth > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_tilde_is_not_strikethrough() {
        let events: Vec<_> = offset_events("~not strike~").map(|(e, _)| e).collect();
        assert!(!events.iter().any(|e| matches!(e, Event::Start(Tag::Strikethrough))));
    }

    #[test]
    fn double_tilde_is_strikethrough() {
        let events: Vec<_> = offset_events("~~strike~~").map(|(e, _)| e).collect();
        assert!(events.iter().any(|e| matches!(e, Event::Start(Tag::Strikethrough))));
    }
}

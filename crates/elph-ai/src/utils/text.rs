//! Text utilities for message content extraction.
//!
//! Ported from `@earendil-works/pi-ai` (`packages/ai/src/utils/text.ts`).

use crate::types::{AssistantContentBlock, ContentBlock};

/// Extract joined text content from message content blocks.
///
/// When `content` is a `String` it is returned as-is.
/// When it is a slice of content blocks, all text blocks are joined with
/// the given `separator` (default `"\n"`).
pub fn content_text(content: &[ContentBlock], separator: &str) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(separator)
}

/// Extract joined text from assistant content blocks.
pub fn assistant_content_text(content: &[AssistantContentBlock], separator: &str) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            AssistantContentBlock::Text(tc) => Some(tc.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(separator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AssistantContentBlock, ContentBlock, TextContent, ThinkingContent};

    #[test]
    fn test_content_text_with_string() {
        // ContentBlock doesn't have a String variant, only Text { text }
        let blocks = vec![ContentBlock::Text {
            text: "hello".to_string(),
        }];
        assert_eq!(content_text(&blocks, "\n"), "hello");
    }

    #[test]
    fn test_content_text_multiple_blocks() {
        let blocks = vec![
            ContentBlock::Text {
                text: "first".to_string(),
            },
            ContentBlock::Text {
                text: "second".to_string(),
            },
        ];
        assert_eq!(content_text(&blocks, "\n"), "first\nsecond");
    }

    #[test]
    fn test_content_text_skips_non_text() {
        let blocks = vec![
            ContentBlock::Text {
                text: "hello".to_string(),
            },
            ContentBlock::Image {
                data: "img".to_string(),
                mime_type: "image/png".to_string(),
            },
            ContentBlock::Text {
                text: "world".to_string(),
            },
        ];
        assert_eq!(content_text(&blocks, " "), "hello world");
    }

    #[test]
    fn test_content_text_empty() {
        assert_eq!(content_text(&[], "\n"), "");
    }

    #[test]
    fn test_assistant_content_text() {
        let blocks = vec![
            AssistantContentBlock::Text(TextContent::new("hello")),
            AssistantContentBlock::Thinking(ThinkingContent::new("think")),
            AssistantContentBlock::Text(TextContent::new("world")),
        ];
        assert_eq!(assistant_content_text(&blocks, " "), "hello world");
    }
}

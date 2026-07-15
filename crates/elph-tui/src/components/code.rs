//! Syntax-highlighted code block (basic token coloring).

use super::line_numbers::LineNumbers;
use iocraft::prelude::*;

/// Props for [`CodeBlock`].
#[derive(Clone, Default, Props)]
pub struct CodeBlockProps {
    pub width: u16,
    pub source: String,
    pub show_line_numbers: bool,
    pub gutter_width: u16,
}

pub fn highlight_rust_line(line: &str) -> Vec<MixedTextContent> {
    let keywords = [
        "fn", "let", "mut", "pub", "use", "struct", "enum", "impl", "return", "if", "else", "match",
    ];
    let mut parts = Vec::new();
    let mut rest = line;

    while !rest.is_empty() {
        let trimmed = rest.trim_start();
        let leading = rest.len() - trimmed.len();
        if leading > 0 {
            parts.push(MixedTextContent::new(" ".repeat(leading)));
            rest = trimmed;
            continue;
        }

        if rest.starts_with("//") {
            parts.push(MixedTextContent::new(rest).color(Color::DarkGrey));
            break;
        }

        if rest.starts_with('"')
            && let Some(end) = rest[1..].find('"')
        {
            let chunk = &rest[..end + 2];
            parts.push(MixedTextContent::new(chunk).color(Color::Green));
            rest = &rest[end + 2..];
            continue;
        }

        if let Some(word_end) = rest.find(|c: char| !c.is_alphanumeric() && c != '_') {
            if word_end == 0 {
                let (ch, tail) = rest.split_at(1);
                parts.push(MixedTextContent::new(ch).color(Color::Grey));
                rest = tail;
                continue;
            }
            let word = &rest[..word_end];
            let color = if keywords.contains(&word) {
                Color::Cyan
            } else {
                Color::Grey
            };
            parts.push(MixedTextContent::new(word).color(color));
            rest = &rest[word_end..];
        } else {
            let word = rest;
            let color = if keywords.contains(&word) {
                Color::Cyan
            } else {
                Color::Grey
            };
            parts.push(MixedTextContent::new(word).color(color));
            break;
        }
    }

    if parts.is_empty() {
        parts.push(MixedTextContent::new(line));
    }

    parts
}

/// Code block with optional line numbers and basic highlighting.
#[component]
pub fn CodeBlock(props: &CodeBlockProps) -> impl Into<AnyElement<'static>> {
    let lines: Vec<&str> = props.source.lines().collect();
    let line_count = lines.len().max(1);

    let mut code_lines = Vec::new();
    for line in &lines {
        code_lines.push(element! {
            MixedText(contents: highlight_rust_line(line), wrap: TextWrap::NoWrap)
        });
    }

    element! {
        View(
            width: props.width,
            flex_direction: FlexDirection::Row,
            background_color: Color::Rgb { r: 30, g: 30, b: 30 },
            padding: 1,
        ) {
            #(if props.show_line_numbers {
                element! {
                    View(width: props.gutter_width.max(4)) {
                        LineNumbers(
                            line_count: line_count,
                            gutter_width: props.gutter_width.max(4),
                        )
                    }
                }
            } else {
                element!(View(width: 0))
            })
            View(flex_direction: FlexDirection::Column, flex_grow: 1f32) {
                #(code_lines)
            }
        }
    }
}

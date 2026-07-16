//! ASCII banner text (FIGlet + compact bitmap fallback).

use figlet_rs::FIGlet;
use iocraft::prelude::*;

/// Props for [`AsciiText`].
#[derive(Clone, Default, Props)]
pub struct AsciiTextProps {
    pub text: String,
    pub use_figlet: bool,
    pub color: Option<Color>,
}

fn bitmap_char(ch: char) -> [&'static str; 5] {
    match ch.to_ascii_uppercase() {
        'A' => [" ███ ", "█   █", "█████", "█   █", "█   █"],
        'B' => ["████ ", "█   █", "████ ", "█   █", "████ "],
        'E' => ["█████", "█    ", "████ ", "█    ", "█████"],
        'L' => ["█    ", "█    ", "█    ", "█    ", "█████"],
        'P' => ["████ ", "█   █", "████ ", "█    ", "█    "],
        'H' => ["█   █", "█   █", "█████", "█   █", "█   █"],
        _ => ["     ", "  █  ", "  █  ", "  █  ", "     "],
    }
}

pub fn render_bitmap(text: &str) -> String {
    let mut lines: Vec<String> = vec![String::new(); 5];
    for ch in text.chars() {
        let glyph = bitmap_char(ch);
        for (i, row) in glyph.iter().enumerate() {
            lines[i].push_str(row);
            lines[i].push(' ');
        }
    }
    lines.join("\n")
}

pub fn render_figlet(text: &str) -> String {
    if let Ok(font) = FIGlet::standard()
        && let Some(figure) = font.convert(text)
    {
        return figure.as_str().to_string();
    }
    render_bitmap(text)
}

/// Large ASCII/FIGlet banner text.
#[component]
pub fn AsciiText(props: &AsciiTextProps) -> impl Into<AnyElement<'static>> {
    let rendered = if props.use_figlet {
        render_figlet(&props.text)
    } else {
        render_bitmap(&props.text)
    };
    let color = props.color.unwrap_or(Color::Cyan);

    let rows: Vec<_> = rendered
        .lines()
        .map(|line| {
            element! {
                Text(content: line.to_string(), color, wrap: TextWrap::NoWrap)
            }
        })
        .collect();

    element! {
        View(flex_direction: FlexDirection::Column) {
            #(rows)
        }
    }
}

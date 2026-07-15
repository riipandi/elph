//! QR code block display.

use iocraft::prelude::*;
use qrcode::QrCode;

/// Props for [`QrCodeView`].
#[derive(Clone, Default, Props)]
pub struct QrCodeViewProps {
    pub payload: String,
    pub dark_char: String,
    pub light_char: String,
    pub color: Option<Color>,
}

pub fn render_qr(payload: &str, dark: &str, light: &str) -> String {
    let Ok(code) = QrCode::new(payload.as_bytes()) else {
        return "invalid payload".to_string();
    };
    let modules = code.to_colors();
    let width = code.width();
    let mut out = String::new();
    for y in 0..width {
        for x in 0..width {
            let idx = y * width + x;
            let ch = if modules[idx] == qrcode::Color::Dark {
                dark
            } else {
                light
            };
            out.push_str(ch);
        }
        out.push('\n');
    }
    out
}

/// QR code rendered as block characters.
#[component]
pub fn QrCodeView(props: &QrCodeViewProps) -> impl Into<AnyElement<'static>> {
    let dark = if props.dark_char.is_empty() {
        "██"
    } else {
        &props.dark_char
    };
    let light = if props.light_char.is_empty() {
        "  "
    } else {
        &props.light_char
    };
    let grid = render_qr(&props.payload, dark, light);
    let color = props.color.unwrap_or(Color::White);

    element! {
        View(flex_direction: FlexDirection::Column) {
            #(grid.lines().map(|line| {
                element! {
                    Text(content: line.to_string(), color, wrap: TextWrap::NoWrap)
                }
            }).collect::<Vec<_>>())
        }
    }
}

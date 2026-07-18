//! Color helpers for terminal UI components.
//!
//! Parses user-facing color strings into iocraft [`Color`]:
//! - Hex: `#RGB`, `#RRGGBB`, `#RRGGBBAA` (alpha ignored)
//! - CSS: `rgb(r, g, b)`, `rgba(r, g, b, a)` (alpha ignored)
//! - JSON-friendly: `r,g,b` (three 0–255 integers)
//! - Named: `reset`, `white`, `black`, `red`, `green`, `blue`, `yellow`,
//!   `cyan`, `magenta`, `grey`/`gray`, `darkgrey`/`darkgray`

use iocraft::prelude::Color;

/// Build an iocraft RGB color.
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}

/// Parse a `#RGB` / `#RRGGBB` / `#RRGGBBAA` hex color into an iocraft RGB color.
pub fn from_hex(hex: &str) -> Option<Color> {
    let hex = hex.trim().trim_start_matches('#');
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(rgb(r, g, b))
        }
        6 | 8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(rgb(r, g, b))
        }
        _ => None,
    }
}

/// Parse a CSS-like `rgb(...)` / `rgba(...)` string.
pub fn from_rgb_fn(input: &str) -> Option<Color> {
    let s = input.trim();
    let lower = s.to_ascii_lowercase();
    let body = if let Some(rest) = lower.strip_prefix("rgba(") {
        rest.strip_suffix(')')?
    } else if let Some(rest) = lower.strip_prefix("rgb(") {
        rest.strip_suffix(')')?
    } else {
        return None;
    };
    let parts: Vec<&str> = body.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let r = parse_channel(parts[0])?;
    let g = parse_channel(parts[1])?;
    let b = parse_channel(parts[2])?;
    Some(rgb(r, g, b))
}

fn parse_channel(s: &str) -> Option<u8> {
    let s = s.trim().trim_end_matches('%');
    if s.contains('.') {
        let f: f64 = s.parse().ok()?;
        if (0.0..=1.0).contains(&f) {
            return Some((f * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        return Some(f.round().clamp(0.0, 255.0) as u8);
    }
    s.parse::<u8>().ok()
}

/// Parse bare `r,g,b` triples (0–255).
pub fn from_csv_rgb(input: &str) -> Option<Color> {
    let parts: Vec<&str> = input.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return None;
    }
    let r = parts[0].parse().ok()?;
    let g = parts[1].parse().ok()?;
    let b = parts[2].parse().ok()?;
    Some(rgb(r, g, b))
}

fn from_named(name: &str) -> Option<Color> {
    match name.trim().to_ascii_lowercase().as_str() {
        "reset" | "default" => Some(Color::Reset),
        "white" => Some(Color::White),
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "blue" => Some(Color::Blue),
        "yellow" => Some(Color::Yellow),
        "cyan" => Some(Color::Cyan),
        "magenta" => Some(Color::Magenta),
        "grey" | "gray" => Some(Color::Grey),
        "darkgrey" | "darkgray" => Some(Color::DarkGrey),
        _ => None,
    }
}

/// Parse any supported color string into an iocraft [`Color`].
///
/// Accepts hex, `rgb()`/`rgba()`, `r,g,b`, and a small set of named colors.
pub fn parse_color(input: &str) -> Option<Color> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(c) = from_named(s) {
        return Some(c);
    }
    if s.starts_with('#') || (s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit())) {
        if let Some(c) = from_hex(s) {
            return Some(c);
        }
    }
    if s.to_ascii_lowercase().starts_with("rgb") {
        if let Some(c) = from_rgb_fn(s) {
            return Some(c);
        }
    }
    if s.contains(',') {
        if let Some(c) = from_csv_rgb(s) {
            return Some(c);
        }
        // Allow `rgb` without function wrapper already handled; try rgba-like csv of 4
        let parts: Vec<&str> = s.split(',').map(str::trim).collect();
        if parts.len() == 4 {
            let r = parse_channel(parts[0])?;
            let g = parse_channel(parts[1])?;
            let b = parse_channel(parts[2])?;
            return Some(rgb(r, g, b));
        }
    }
    None
}

/// Parse a color from a JSON value: string forms or `{ "r", "g", "b" }` object.
pub fn parse_color_value(value: &serde_json::Value) -> Option<Color> {
    match value {
        serde_json::Value::String(s) => parse_color(s),
        serde_json::Value::Array(arr) if arr.len() >= 3 => {
            let r = arr[0].as_u64().or_else(|| arr[0].as_f64().map(|f| f as u64))? as u8;
            let g = arr[1].as_u64().or_else(|| arr[1].as_f64().map(|f| f as u64))? as u8;
            let b = arr[2].as_u64().or_else(|| arr[2].as_f64().map(|f| f as u64))? as u8;
            Some(rgb(r, g, b))
        }
        serde_json::Value::Object(map) => {
            let r = channel_from_map(map, "r").or_else(|| channel_from_map(map, "red"))?;
            let g = channel_from_map(map, "g").or_else(|| channel_from_map(map, "green"))?;
            let b = channel_from_map(map, "b").or_else(|| channel_from_map(map, "blue"))?;
            Some(rgb(r, g, b))
        }
        _ => None,
    }
}

fn channel_from_map(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<u8> {
    let v = map.get(key)?;
    if let Some(n) = v.as_u64() {
        return u8::try_from(n).ok();
    }
    if let Some(f) = v.as_f64() {
        return Some(f.round().clamp(0.0, 255.0) as u8);
    }
    if let Some(s) = v.as_str() {
        return parse_channel(s);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert_eq!(from_hex("#fff"), Some(rgb(255, 255, 255)));
        assert_eq!(from_hex("#d4d5d9"), Some(rgb(0xd4, 0xd5, 0xd9)));
        assert_eq!(from_hex("#6699ffff"), Some(rgb(0x66, 0x99, 0xff)));
        assert_eq!(from_hex("8ed16a"), Some(rgb(0x8e, 0xd1, 0x6a)));
    }

    #[test]
    fn parses_rgb_functions() {
        assert_eq!(from_rgb_fn("rgb(102, 153, 255)"), Some(rgb(102, 153, 255)));
        assert_eq!(from_rgb_fn("rgba(255,107,102,0.5)"), Some(rgb(255, 107, 102)));
        assert_eq!(from_rgb_fn("rgb(0.5, 0, 1.0)"), Some(rgb(128, 0, 255)));
    }

    #[test]
    fn parses_csv_and_named() {
        assert_eq!(from_csv_rgb("18, 26, 29"), Some(rgb(18, 26, 29)));
        assert_eq!(parse_color("white"), Some(Color::White));
        assert_eq!(parse_color("reset"), Some(Color::Reset));
    }

    #[test]
    fn parse_color_value_object_and_array() {
        let obj = serde_json::json!({ "r": 102, "g": 153, "b": 255 });
        assert_eq!(parse_color_value(&obj), Some(rgb(102, 153, 255)));
        let arr = serde_json::json!([142, 209, 106]);
        assert_eq!(parse_color_value(&arr), Some(rgb(142, 209, 106)));
        let s = serde_json::json!("#ff6b66");
        assert_eq!(parse_color_value(&s), Some(rgb(0xff, 0x6b, 0x66)));
    }
}
